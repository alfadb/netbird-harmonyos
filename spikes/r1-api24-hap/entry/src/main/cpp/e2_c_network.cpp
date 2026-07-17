#include <arpa/inet.h>
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <hilog/log.h>
#include <ifaddrs.h>
#include <napi/native_api.h>
#include <net/if.h>
#include <netdb.h>
#include <poll.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/epoll.h>
#include <sys/socket.h>
#include <sys/syscall.h>
#include <sys/types.h>
#include <time.h>
#include <unistd.h>

namespace {
constexpr unsigned int kLogDomain = 0x2900;
constexpr const char *kLogTag = "R1Api24Probe";
constexpr const char *kProbeVersion = "e2-c-network-api24-probe/0.0.1";
constexpr int kIoTimeoutMs = 2000;
constexpr int kExpectedTimeoutMs = 25;
constexpr uint16_t kHostTcpPort = 39021;
constexpr uint16_t kHostUdpPort = 39022;
constexpr size_t kTcpPayloadLength = 4096;
constexpr size_t kHostTcpPayloadLength = 2048;
constexpr size_t kUdpPayloadLength = 192;
constexpr size_t kHostUdpPayloadLength = 224;
constexpr int kUdpDatagramsPerDirection = 3;
constexpr int kHostUdpDatagrams = 3;

struct TcpLoopResult {
    uint32_t sentHash;
    uint32_t echoHash;
    int bytes;
    int clientWriteCalls;
    int serverReadCalls;
    int serverWriteCalls;
    int clientReadCalls;
    int epollEvents;
    int timeoutElapsedMs;
    int peerCloseEvents;
    int refusedErrno;
    int listenerCloseErrno;
    int listenerPort;
};

struct UdpLoopResult {
    uint32_t clientAggregate;
    uint32_t serverAggregate;
    int clientToServer;
    int serverToClient;
    int pollEvents;
};

struct HostResult {
    char route[128];
    char address[INET_ADDRSTRLEN];
    uint32_t tcpHash;
    uint32_t udpAggregate;
    int tcpBytes;
    int tcpWriteCalls;
    int tcpReadCalls;
    int tcpPollEvents;
    int udpDatagrams;
    int udpPollEvents;
    int candidateAttempts;
};

struct DnsResult {
    int localhostCount;
    int ipv4Count;
    int ipv6Count;
    int invalidCode;
    char invalidMessage[128];
};

struct RouteCandidates {
    in_addr addresses[8];
    int count;
    char route[128];
    char source[32];
};

int CurrentTid() {
    return static_cast<int>(syscall(SYS_gettid));
}

uint32_t Fnv1a(const uint8_t *data, size_t length) {
    uint32_t hash = 2166136261U;
    for (size_t index = 0; index < length; ++index) {
        hash ^= data[index];
        hash *= 16777619U;
    }
    return hash;
}

uint32_t MixHash(uint32_t aggregate, uint32_t hash, uint32_t id) {
    aggregate ^= hash;
    aggregate *= 16777619U;
    aggregate ^= id;
    aggregate *= 16777619U;
    return aggregate;
}

void SetError(char *error, size_t capacity, const char *operation, int code) {
    snprintf(error, capacity, "%s failed code=%d errno=%d", operation, code, errno);
}

void SetMessage(char *error, size_t capacity, const char *message) {
    snprintf(error, capacity, "%s", message);
}

void CloseFd(int *fd) {
    if (*fd >= 0) {
        close(*fd);
        *fd = -1;
    }
}

int MonotonicMilliseconds() {
    timespec value = {};
    if (clock_gettime(CLOCK_MONOTONIC, &value) != 0) {
        return -1;
    }
    return static_cast<int>(value.tv_sec * 1000 + value.tv_nsec / 1000000);
}

bool WaitFd(int fd, short events, int timeoutMs, short *revents, int *pollEvents,
    char *error, size_t capacity) {
    pollfd descriptor = {fd, events, 0};
    int status;
    do {
        status = poll(&descriptor, 1, timeoutMs);
    } while (status < 0 && errno == EINTR);
    if (status != 1) {
        snprintf(error, capacity, "poll readiness failed fd=%d status=%d timeout=%d errno=%d",
            fd, status, timeoutMs, errno);
        return false;
    }
    if ((descriptor.revents & POLLNVAL) != 0) {
        snprintf(error, capacity, "poll returned POLLNVAL fd=%d", fd);
        return false;
    }
    *revents = descriptor.revents;
    if (pollEvents != nullptr) {
        ++(*pollEvents);
    }
    return true;
}

bool SendChunked(int fd, const uint8_t *data, size_t length, const size_t *chunks, size_t chunkCount,
    int *calls, int *pollEvents, char *error, size_t capacity) {
    size_t offset = 0;
    size_t chunkIndex = 0;
    while (offset < length) {
        short revents = 0;
        if (!WaitFd(fd, POLLOUT, kIoTimeoutMs, &revents, pollEvents, error, capacity) ||
            (revents & (POLLOUT | POLLERR | POLLHUP)) == 0) {
            if (error[0] == '\0') {
                SetMessage(error, capacity, "socket never became writable");
            }
            return false;
        }
        size_t requested = chunks[chunkIndex % chunkCount];
        if (requested > length - offset) {
            requested = length - offset;
        }
        const ssize_t written = send(fd, data + offset, requested, MSG_NOSIGNAL);
        if (written < 0 && (errno == EINTR || errno == EAGAIN || errno == EWOULDBLOCK)) {
            continue;
        }
        if (written <= 0) {
            SetError(error, capacity, "send", static_cast<int>(written));
            return false;
        }
        offset += static_cast<size_t>(written);
        ++(*calls);
        ++chunkIndex;
    }
    return true;
}

bool ReadExactPartial(int fd, uint8_t *data, size_t length, size_t maxRead, int *calls, int *pollEvents,
    char *error, size_t capacity) {
    size_t offset = 0;
    while (offset < length) {
        short revents = 0;
        if (!WaitFd(fd, POLLIN, kIoTimeoutMs, &revents, pollEvents, error, capacity) ||
            (revents & (POLLIN | POLLERR | POLLHUP | POLLRDHUP)) == 0) {
            if (error[0] == '\0') {
                SetMessage(error, capacity, "socket never became readable");
            }
            return false;
        }
        size_t requested = maxRead;
        if (requested > length - offset) {
            requested = length - offset;
        }
        const ssize_t received = recv(fd, data + offset, requested, 0);
        if (received < 0 && (errno == EINTR || errno == EAGAIN || errno == EWOULDBLOCK)) {
            continue;
        }
        if (received <= 0) {
            SetError(error, capacity, "recv exact", static_cast<int>(received));
            return false;
        }
        offset += static_cast<size_t>(received);
        ++(*calls);
    }
    return true;
}

void FillPayload(uint8_t *payload, size_t length, int round, int salt) {
    for (size_t index = 0; index < length; ++index) {
        payload[index] = static_cast<uint8_t>((round * 37 + salt * 53 + static_cast<int>(index) * 17 +
            static_cast<int>(index >> 3)) & 0xff);
    }
}

bool AddEpollFd(int epollFd, int fd, uint32_t events, char *error, size_t capacity) {
    epoll_event event = {};
    event.events = events;
    event.data.fd = fd;
    if (epoll_ctl(epollFd, EPOLL_CTL_ADD, fd, &event) != 0) {
        SetError(error, capacity, "epoll_ctl add", -1);
        return false;
    }
    return true;
}

bool RunTcpLoopback(int round, TcpLoopResult *result, char *error, size_t capacity) {
    int listener = -1;
    int client = -1;
    int server = -1;
    int epollFd = -1;
    int refused = -1;
    bool success = false;
    sockaddr_in listenerAddress = {};
    socklen_t addressLength = sizeof(listenerAddress);
    int connectStatus = -1;
    bool connected = false;

    listener = socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC | SOCK_NONBLOCK, 0);
    if (listener < 0) {
        SetError(error, capacity, "TCP listener socket", listener);
        goto cleanup;
    }
    listenerAddress.sin_family = AF_INET;
    listenerAddress.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    listenerAddress.sin_port = 0;
    if (bind(listener, reinterpret_cast<sockaddr *>(&listenerAddress), sizeof(listenerAddress)) != 0 ||
        listen(listener, 4) != 0) {
        SetError(error, capacity, "TCP bind/listen", -1);
        goto cleanup;
    }
    if (getsockname(listener, reinterpret_cast<sockaddr *>(&listenerAddress), &addressLength) != 0) {
        SetError(error, capacity, "TCP getsockname", -1);
        goto cleanup;
    }
    result->listenerPort = ntohs(listenerAddress.sin_port);

    epollFd = epoll_create1(EPOLL_CLOEXEC);
    if (epollFd < 0 || !AddEpollFd(epollFd, listener, EPOLLIN, error, capacity)) {
        if (error[0] == '\0') {
            SetError(error, capacity, "epoll_create1", epollFd);
        }
        goto cleanup;
    }
    client = socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC | SOCK_NONBLOCK, 0);
    if (client < 0 || !AddEpollFd(epollFd, client, EPOLLOUT | EPOLLIN | EPOLLRDHUP, error, capacity)) {
        if (error[0] == '\0') {
            SetError(error, capacity, "TCP client socket", client);
        }
        goto cleanup;
    }
    connectStatus = connect(client, reinterpret_cast<sockaddr *>(&listenerAddress), sizeof(listenerAddress));
    if (connectStatus != 0 && errno != EINPROGRESS) {
        SetError(error, capacity, "TCP loopback connect", connectStatus);
        goto cleanup;
    }

    connected = connectStatus == 0;
    for (int attempt = 0; attempt < 8 && (!connected || server < 0); ++attempt) {
        epoll_event events[4] = {};
        int eventCount;
        do {
            eventCount = epoll_wait(epollFd, events, 4, kIoTimeoutMs);
        } while (eventCount < 0 && errno == EINTR);
        if (eventCount <= 0) {
            snprintf(error, capacity, "epoll connect/accept timeout status=%d errno=%d", eventCount, errno);
            goto cleanup;
        }
        result->epollEvents += eventCount;
        for (int index = 0; index < eventCount; ++index) {
            if (events[index].data.fd == listener && (events[index].events & EPOLLIN) != 0 && server < 0) {
                server = accept4(listener, nullptr, nullptr, SOCK_CLOEXEC | SOCK_NONBLOCK);
                if (server < 0 && errno != EAGAIN && errno != EWOULDBLOCK) {
                    SetError(error, capacity, "accept4", server);
                    goto cleanup;
                }
                if (server >= 0 && !AddEpollFd(epollFd, server, EPOLLIN | EPOLLRDHUP, error, capacity)) {
                    goto cleanup;
                }
            }
            if (events[index].data.fd == client &&
                (events[index].events & (EPOLLOUT | EPOLLERR | EPOLLHUP)) != 0) {
                int socketError = 0;
                socklen_t socketErrorLength = sizeof(socketError);
                if (getsockopt(client, SOL_SOCKET, SO_ERROR, &socketError, &socketErrorLength) != 0 ||
                    socketError != 0) {
                    errno = socketError;
                    SetError(error, capacity, "TCP loopback SO_ERROR", socketError);
                    goto cleanup;
                }
                connected = true;
            }
        }
    }
    if (!connected || server < 0) {
        SetMessage(error, capacity, "TCP loopback did not connect and accept");
        goto cleanup;
    }

    {
        pollfd idle = {server, POLLIN, 0};
        const int started = MonotonicMilliseconds();
        int timeoutStatus;
        do {
            timeoutStatus = poll(&idle, 1, kExpectedTimeoutMs);
        } while (timeoutStatus < 0 && errno == EINTR);
        const int ended = MonotonicMilliseconds();
        result->timeoutElapsedMs = started < 0 || ended < 0 ? -1 : ended - started;
        if (timeoutStatus != 0 || result->timeoutElapsedMs < 15) {
            snprintf(error, capacity, "poll timeout path failed status=%d elapsed=%d revents=%d",
                timeoutStatus, result->timeoutElapsedMs, idle.revents);
            goto cleanup;
        }
    }

    {
        uint8_t sent[kTcpPayloadLength] = {};
        uint8_t receivedByServer[kTcpPayloadLength] = {};
        uint8_t receivedByClient[kTcpPayloadLength] = {};
        const size_t clientChunks[] = {257, 31, 509, 73, 1021, 19};
        const size_t serverChunks[] = {211, 43, 887, 29, 601};
        int ignoredPollEvents = 0;
        FillPayload(sent, sizeof(sent), round, 1);
        result->sentHash = Fnv1a(sent, sizeof(sent));
        if (!SendChunked(client, sent, sizeof(sent), clientChunks,
                sizeof(clientChunks) / sizeof(clientChunks[0]), &result->clientWriteCalls,
                &ignoredPollEvents, error, capacity) ||
            !ReadExactPartial(server, receivedByServer, sizeof(receivedByServer), 29,
                &result->serverReadCalls, &ignoredPollEvents, error, capacity) ||
            memcmp(sent, receivedByServer, sizeof(sent)) != 0 ||
            !SendChunked(server, receivedByServer, sizeof(receivedByServer), serverChunks,
                sizeof(serverChunks) / sizeof(serverChunks[0]), &result->serverWriteCalls,
                &ignoredPollEvents, error, capacity) ||
            !ReadExactPartial(client, receivedByClient, sizeof(receivedByClient), 23,
                &result->clientReadCalls, &ignoredPollEvents, error, capacity) ||
            memcmp(sent, receivedByClient, sizeof(sent)) != 0) {
            if (error[0] == '\0') {
                SetMessage(error, capacity, "TCP loopback payload/echo mismatch");
            }
            goto cleanup;
        }
        result->echoHash = Fnv1a(receivedByClient, sizeof(receivedByClient));
        result->bytes = static_cast<int>(sizeof(sent));
        if (result->sentHash != result->echoHash || result->serverReadCalls <= result->clientWriteCalls ||
            result->clientReadCalls <= result->serverWriteCalls) {
            SetMessage(error, capacity, "TCP partial-read or hash assertion failed");
            goto cleanup;
        }
    }

    CloseFd(&client);
    for (int attempt = 0; attempt < 5 && result->peerCloseEvents == 0; ++attempt) {
        epoll_event events[4] = {};
        int eventCount;
        do {
            eventCount = epoll_wait(epollFd, events, 4, kIoTimeoutMs);
        } while (eventCount < 0 && errno == EINTR);
        if (eventCount <= 0) {
            snprintf(error, capacity, "epoll peer-close timeout status=%d errno=%d", eventCount, errno);
            goto cleanup;
        }
        result->epollEvents += eventCount;
        for (int index = 0; index < eventCount; ++index) {
            if (events[index].data.fd == server &&
                (events[index].events & (EPOLLIN | EPOLLRDHUP | EPOLLHUP | EPOLLERR)) != 0) {
                uint8_t byte = 0;
                const ssize_t closeRead = recv(server, &byte, 1, 0);
                if (closeRead != 0) {
                    snprintf(error, capacity, "TCP peer-close read was not EOF result=%zd errno=%d", closeRead, errno);
                    goto cleanup;
                }
                result->peerCloseEvents = 1;
            }
        }
    }
    if (result->peerCloseEvents != 1) {
        SetMessage(error, capacity, "TCP peer-close event missing");
        goto cleanup;
    }

    CloseFd(&server);
    {
        const int closedListener = listener;
        CloseFd(&listener);
        errno = 0;
        if (fcntl(closedListener, F_GETFD) != -1 || errno != EBADF) {
            SetMessage(error, capacity, "closed TCP listener did not report EBADF");
            goto cleanup;
        }
        result->listenerCloseErrno = errno;
    }
    CloseFd(&epollFd);

    refused = socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC | SOCK_NONBLOCK, 0);
    if (refused < 0) {
        SetError(error, capacity, "TCP refused socket", refused);
        goto cleanup;
    }
    connectStatus = connect(refused, reinterpret_cast<sockaddr *>(&listenerAddress), sizeof(listenerAddress));
    if (connectStatus == 0) {
        SetMessage(error, capacity, "connection unexpectedly succeeded after listener close");
        goto cleanup;
    }
    if (errno == ECONNREFUSED) {
        result->refusedErrno = errno;
    } else if (errno == EINPROGRESS) {
        short revents = 0;
        int ignoredPollEvents = 0;
        if (!WaitFd(refused, POLLOUT | POLLERR, kIoTimeoutMs, &revents, &ignoredPollEvents,
                error, capacity)) {
            goto cleanup;
        }
        int socketError = 0;
        socklen_t socketErrorLength = sizeof(socketError);
        if (getsockopt(refused, SOL_SOCKET, SO_ERROR, &socketError, &socketErrorLength) != 0) {
            SetError(error, capacity, "refused SO_ERROR", -1);
            goto cleanup;
        }
        result->refusedErrno = socketError;
    } else {
        result->refusedErrno = errno;
    }
    if (result->refusedErrno != ECONNREFUSED) {
        snprintf(error, capacity, "closed listener error was not ECONNREFUSED actual=%d", result->refusedErrno);
        goto cleanup;
    }

    success = true;

cleanup:
    CloseFd(&refused);
    CloseFd(&server);
    CloseFd(&client);
    CloseFd(&listener);
    CloseFd(&epollFd);
    return success;
}

bool BindUdpLoopback(int *fd, sockaddr_in *address, char *error, size_t capacity) {
    *fd = socket(AF_INET, SOCK_DGRAM | SOCK_CLOEXEC | SOCK_NONBLOCK, 0);
    if (*fd < 0) {
        SetError(error, capacity, "UDP socket", *fd);
        return false;
    }
    address->sin_family = AF_INET;
    address->sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    address->sin_port = 0;
    if (bind(*fd, reinterpret_cast<sockaddr *>(address), sizeof(*address)) != 0) {
        SetError(error, capacity, "UDP bind", -1);
        return false;
    }
    socklen_t length = sizeof(*address);
    if (getsockname(*fd, reinterpret_cast<sockaddr *>(address), &length) != 0) {
        SetError(error, capacity, "UDP getsockname", -1);
        return false;
    }
    return true;
}

void FillDatagram(uint8_t *payload, size_t length, int round, const char *direction, int id, int salt) {
    memset(payload, 0, length);
    const int headerLength = snprintf(reinterpret_cast<char *>(payload), length,
        "E2UDP|round=%02d|dir=%s|id=%04d|", round, direction, id);
    if (headerLength < 0 || static_cast<size_t>(headerLength) >= length) {
        return;
    }
    for (size_t index = static_cast<size_t>(headerLength); index < length; ++index) {
        payload[index] = static_cast<uint8_t>((round * 19 + id * 7 + salt * 43 +
            static_cast<int>(index) * 11) & 0xff);
    }
}

bool SendDatagram(int fd, const sockaddr_in &destination, const uint8_t *payload, size_t length,
    int *pollEvents, char *error, size_t capacity) {
    short revents = 0;
    if (!WaitFd(fd, POLLOUT, kIoTimeoutMs, &revents, pollEvents, error, capacity)) {
        return false;
    }
    const ssize_t sent = sendto(fd, payload, length, 0,
        reinterpret_cast<const sockaddr *>(&destination), sizeof(destination));
    if (sent != static_cast<ssize_t>(length)) {
        SetError(error, capacity, "sendto datagram", static_cast<int>(sent));
        return false;
    }
    return true;
}

bool ReceiveDatagram(int fd, const sockaddr_in &expectedSource, uint8_t *payload, size_t capacityBytes,
    size_t expectedLength, int *pollEvents, char *error, size_t errorCapacity) {
    short revents = 0;
    if (!WaitFd(fd, POLLIN, kIoTimeoutMs, &revents, pollEvents, error, errorCapacity)) {
        return false;
    }
    sockaddr_in source = {};
    socklen_t sourceLength = sizeof(source);
    const ssize_t received = recvfrom(fd, payload, capacityBytes, 0,
        reinterpret_cast<sockaddr *>(&source), &sourceLength);
    if (received != static_cast<ssize_t>(expectedLength) || source.sin_family != AF_INET ||
        source.sin_addr.s_addr != expectedSource.sin_addr.s_addr || source.sin_port != expectedSource.sin_port) {
        snprintf(error, errorCapacity,
            "recvfrom datagram mismatch bytes=%zd family=%d sourcePort=%u expectedPort=%u errno=%d",
            received, source.sin_family, ntohs(source.sin_port), ntohs(expectedSource.sin_port), errno);
        return false;
    }
    return true;
}

bool RunUdpLoopback(int round, UdpLoopResult *result, char *error, size_t capacity) {
    int client = -1;
    int server = -1;
    sockaddr_in clientAddress = {};
    sockaddr_in serverAddress = {};
    bool success = false;
    if (!BindUdpLoopback(&client, &clientAddress, error, capacity) ||
        !BindUdpLoopback(&server, &serverAddress, error, capacity)) {
        goto cleanup;
    }

    result->clientAggregate = 2166136261U;
    result->serverAggregate = 2166136261U;
    for (int slot = 1; slot <= kUdpDatagramsPerDirection; ++slot) {
        const int clientId = round * 100 + slot;
        uint8_t sent[kUdpPayloadLength] = {};
        uint8_t received[kUdpPayloadLength + 1] = {};
        FillDatagram(sent, sizeof(sent), round, "C2S", clientId, 1);
        const uint32_t sentHash = Fnv1a(sent, sizeof(sent));
        if (!SendDatagram(client, serverAddress, sent, sizeof(sent), &result->pollEvents, error, capacity) ||
            !ReceiveDatagram(server, clientAddress, received, sizeof(received), sizeof(sent),
                &result->pollEvents, error, capacity) ||
            memcmp(sent, received, sizeof(sent)) != 0 || Fnv1a(received, sizeof(sent)) != sentHash) {
            if (error[0] == '\0') {
                SetMessage(error, capacity, "UDP client-to-server datagram mismatch");
            }
            goto cleanup;
        }
        result->clientAggregate = MixHash(result->clientAggregate, sentHash, static_cast<uint32_t>(clientId));
        ++result->clientToServer;
        OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
            "E2_UDP_DATAGRAM|round=%{public}d|direction=C2S|id=%{public}d|bytes=%{public}zu|hash=%{public}u|valid=true",
            round, clientId, sizeof(sent), sentHash);

        const int serverId = round * 100 + 50 + slot;
        memset(sent, 0, sizeof(sent));
        memset(received, 0, sizeof(received));
        FillDatagram(sent, sizeof(sent), round, "S2C", serverId, 2);
        const uint32_t responseHash = Fnv1a(sent, sizeof(sent));
        if (!SendDatagram(server, clientAddress, sent, sizeof(sent), &result->pollEvents, error, capacity) ||
            !ReceiveDatagram(client, serverAddress, received, sizeof(received), sizeof(sent),
                &result->pollEvents, error, capacity) ||
            memcmp(sent, received, sizeof(sent)) != 0 || Fnv1a(received, sizeof(sent)) != responseHash) {
            if (error[0] == '\0') {
                SetMessage(error, capacity, "UDP server-to-client datagram mismatch");
            }
            goto cleanup;
        }
        result->serverAggregate = MixHash(result->serverAggregate, responseHash, static_cast<uint32_t>(serverId));
        ++result->serverToClient;
        OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
            "E2_UDP_DATAGRAM|round=%{public}d|direction=S2C|id=%{public}d|bytes=%{public}zu|hash=%{public}u|valid=true",
            round, serverId, sizeof(sent), responseHash);
    }
    success = result->clientToServer == kUdpDatagramsPerDirection &&
        result->serverToClient == kUdpDatagramsPerDirection;
    if (!success) {
        SetMessage(error, capacity, "UDP loopback datagram count mismatch");
    }

cleanup:
    CloseFd(&server);
    CloseFd(&client);
    return success;
}

bool AddCandidate(RouteCandidates *candidates, uint32_t networkOrderAddress) {
    if (networkOrderAddress == 0 || networkOrderAddress == htonl(INADDR_LOOPBACK)) {
        return true;
    }
    for (int index = 0; index < candidates->count; ++index) {
        if (candidates->addresses[index].s_addr == networkOrderAddress) {
            return true;
        }
    }
    if (candidates->count >= static_cast<int>(sizeof(candidates->addresses) / sizeof(candidates->addresses[0]))) {
        return false;
    }
    candidates->addresses[candidates->count++].s_addr = networkOrderAddress;
    return true;
}

bool DiscoverInterfaceCandidates(RouteCandidates *candidates, char *error, size_t capacity) {
    ifaddrs *interfaces = nullptr;
    if (getifaddrs(&interfaces) != 0 || interfaces == nullptr) {
        SetError(error, capacity, "getifaddrs", -1);
        return false;
    }
    for (const ifaddrs *entry = interfaces; entry != nullptr; entry = entry->ifa_next) {
        if (entry->ifa_addr == nullptr || entry->ifa_netmask == nullptr ||
            entry->ifa_addr->sa_family != AF_INET || (entry->ifa_flags & IFF_UP) == 0 ||
            (entry->ifa_flags & IFF_LOOPBACK) != 0) {
            continue;
        }
        const auto *address = reinterpret_cast<const sockaddr_in *>(entry->ifa_addr);
        const auto *netmask = reinterpret_cast<const sockaddr_in *>(entry->ifa_netmask);
        const uint32_t addressHost = ntohl(address->sin_addr.s_addr);
        const uint32_t maskHost = ntohl(netmask->sin_addr.s_addr);
        if (maskHost == 0) {
            continue;
        }
        const uint32_t networkHost = addressHost & maskHost;
        const uint32_t broadcastHost = networkHost | ~maskHost;
        if (networkHost + 2U < broadcastHost && !AddCandidate(candidates, htonl(networkHost + 2U))) {
            freeifaddrs(interfaces);
            SetMessage(error, capacity, "too many getifaddrs route candidates");
            return false;
        }
        if (networkHost + 1U < broadcastHost && !AddCandidate(candidates, htonl(networkHost + 1U))) {
            freeifaddrs(interfaces);
            SetMessage(error, capacity, "too many getifaddrs route candidates");
            return false;
        }
        if (candidates->route[0] == '\0') {
            in_addr networkAddress = {htonl(networkHost)};
            char networkText[INET_ADDRSTRLEN] = {};
            char maskText[INET_ADDRSTRLEN] = {};
            inet_ntop(AF_INET, &networkAddress, networkText, sizeof(networkText));
            inet_ntop(AF_INET, &netmask->sin_addr, maskText, sizeof(maskText));
            snprintf(candidates->route, sizeof(candidates->route), "%s:%s/%s",
                entry->ifa_name, networkText, maskText);
        }
    }
    freeifaddrs(interfaces);
    if (candidates->count == 0 || candidates->route[0] == '\0') {
        SetMessage(error, capacity, "getifaddrs found no non-loopback IPv4 network");
        return false;
    }
    snprintf(candidates->source, sizeof(candidates->source), "getifaddrs");
    return true;
}

bool DiscoverRouteCandidates(RouteCandidates *candidates, char *error, size_t capacity) {
    FILE *routes = fopen("/proc/net/route", "re");
    if (routes == nullptr) {
        return DiscoverInterfaceCandidates(candidates, error, capacity);
    }
    char line[512] = {};
    if (fgets(line, sizeof(line), routes) == nullptr) {
        fclose(routes);
        SetMessage(error, capacity, "empty /proc/net/route");
        return false;
    }

    struct DirectRoute {
        char interfaceName[16];
        uint32_t destination;
        uint32_t mask;
    } directRoutes[8] = {};
    int directCount = 0;
    while (fgets(line, sizeof(line), routes) != nullptr) {
        char interfaceName[16] = {};
        unsigned long destination = 0;
        unsigned long gateway = 0;
        unsigned int flags = 0;
        unsigned long refCount = 0;
        unsigned long use = 0;
        unsigned long metric = 0;
        unsigned long mask = 0;
        const int fields = sscanf(line, "%15s %lx %lx %x %lu %lu %lu %lx",
            interfaceName, &destination, &gateway, &flags, &refCount, &use, &metric, &mask);
        if (fields != 8 || (flags & 0x1U) == 0 || strcmp(interfaceName, "lo") == 0) {
            continue;
        }
        if (destination == 0 && gateway != 0) {
            if (!AddCandidate(candidates, static_cast<uint32_t>(gateway))) {
                fclose(routes);
                SetMessage(error, capacity, "too many route gateway candidates");
                return false;
            }
        } else if (destination != 0 && mask != 0 && directCount < 8) {
            snprintf(directRoutes[directCount].interfaceName,
                sizeof(directRoutes[directCount].interfaceName), "%s", interfaceName);
            directRoutes[directCount].destination = static_cast<uint32_t>(destination);
            directRoutes[directCount].mask = static_cast<uint32_t>(mask);
            ++directCount;
        }
    }
    fclose(routes);

    for (int index = 0; index < directCount; ++index) {
        const uint32_t networkHost = ntohl(directRoutes[index].destination) & ntohl(directRoutes[index].mask);
        const uint32_t maskHost = ntohl(directRoutes[index].mask);
        const uint32_t broadcastHost = networkHost | ~maskHost;
        const uint32_t candidateTwo = networkHost + 2U;
        const uint32_t candidateOne = networkHost + 1U;
        if (candidateTwo < broadcastHost && !AddCandidate(candidates, htonl(candidateTwo))) {
            SetMessage(error, capacity, "too many direct route candidates");
            return false;
        }
        if (candidateOne < broadcastHost && !AddCandidate(candidates, htonl(candidateOne))) {
            SetMessage(error, capacity, "too many direct route candidates");
            return false;
        }
        if (candidates->route[0] == '\0') {
            in_addr networkAddress = {htonl(networkHost)};
            in_addr maskAddress = {directRoutes[index].mask};
            char networkText[INET_ADDRSTRLEN] = {};
            char maskText[INET_ADDRSTRLEN] = {};
            inet_ntop(AF_INET, &networkAddress, networkText, sizeof(networkText));
            inet_ntop(AF_INET, &maskAddress, maskText, sizeof(maskText));
            snprintf(candidates->route, sizeof(candidates->route), "%s:%s/%s",
                directRoutes[index].interfaceName, networkText, maskText);
        }
    }
    if (candidates->count == 0 || candidates->route[0] == '\0') {
        memset(candidates, 0, sizeof(*candidates));
        return DiscoverInterfaceCandidates(candidates, error, capacity);
    }
    snprintf(candidates->source, sizeof(candidates->source), "proc_net_route");
    return true;
}

int ConnectCandidate(const in_addr &address, uint16_t port, int *pollEvents, int *connectError,
    char *error, size_t capacity) {
    int fd = socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC | SOCK_NONBLOCK, 0);
    if (fd < 0) {
        SetError(error, capacity, "host TCP socket", fd);
        *connectError = errno;
        return -1;
    }
    sockaddr_in destination = {};
    destination.sin_family = AF_INET;
    destination.sin_addr = address;
    destination.sin_port = htons(port);
    const int status = connect(fd, reinterpret_cast<sockaddr *>(&destination), sizeof(destination));
    if (status == 0) {
        *connectError = 0;
        return fd;
    }
    if (errno != EINPROGRESS) {
        *connectError = errno;
        CloseFd(&fd);
        return -1;
    }
    short revents = 0;
    if (!WaitFd(fd, POLLOUT | POLLERR, 1500, &revents, pollEvents, error, capacity)) {
        *connectError = ETIMEDOUT;
        error[0] = '\0';
        CloseFd(&fd);
        return -1;
    }
    int socketError = 0;
    socklen_t socketErrorLength = sizeof(socketError);
    if (getsockopt(fd, SOL_SOCKET, SO_ERROR, &socketError, &socketErrorLength) != 0) {
        *connectError = errno;
        CloseFd(&fd);
        return -1;
    }
    if (socketError != 0) {
        *connectError = socketError;
        CloseFd(&fd);
        return -1;
    }
    *connectError = 0;
    return fd;
}

void FillHostTcpPayload(uint8_t *payload, size_t length, int round) {
    FillPayload(payload, length, round, 7);
    snprintf(reinterpret_cast<char *>(payload), length,
        "E2HOSTTCP|pid=%d|round=%02d|id=%04d|", getpid(), round, round * 100 + 7);
}

bool RunHostTcp(int round, const RouteCandidates &candidates, HostResult *result,
    char *error, size_t capacity) {
    int fd = -1;
    for (int index = 0; index < candidates.count; ++index) {
        char addressText[INET_ADDRSTRLEN] = {};
        inet_ntop(AF_INET, &candidates.addresses[index], addressText, sizeof(addressText));
        int connectError = 0;
        ++result->candidateAttempts;
        fd = ConnectCandidate(candidates.addresses[index], kHostTcpPort, &result->tcpPollEvents,
            &connectError, error, capacity);
        OH_LOG_Print(LOG_APP, fd >= 0 ? LOG_INFO : LOG_WARN, kLogDomain, kLogTag,
            "E2_HOST_CANDIDATE|round=%{public}d|route=%{public}s|address=%{public}s|port=%{public}u|attempt=%{public}d|errno=%{public}d|connected=%{public}s",
            round, candidates.route, addressText, kHostTcpPort, result->candidateAttempts,
            connectError, fd >= 0 ? "true" : "false");
        if (fd >= 0) {
            snprintf(result->address, sizeof(result->address), "%s", addressText);
            break;
        }
    }
    if (fd < 0) {
        SetMessage(error, capacity, "no route-derived host TCP candidate connected");
        return false;
    }

    uint8_t sent[kHostTcpPayloadLength] = {};
    uint8_t received[kHostTcpPayloadLength] = {};
    const size_t chunks[] = {127, 509, 31, 887, 61};
    FillHostTcpPayload(sent, sizeof(sent), round);
    result->tcpHash = Fnv1a(sent, sizeof(sent));
    bool success = SendChunked(fd, sent, sizeof(sent), chunks, sizeof(chunks) / sizeof(chunks[0]),
        &result->tcpWriteCalls, &result->tcpPollEvents, error, capacity) &&
        ReadExactPartial(fd, received, sizeof(received), 37, &result->tcpReadCalls,
            &result->tcpPollEvents, error, capacity) &&
        memcmp(sent, received, sizeof(sent)) == 0 && Fnv1a(received, sizeof(received)) == result->tcpHash;
    result->tcpBytes = static_cast<int>(sizeof(sent));
    CloseFd(&fd);
    if (!success && error[0] == '\0') {
        SetMessage(error, capacity, "host TCP echo/hash mismatch");
    }
    return success;
}

void FillHostUdpPayload(uint8_t *payload, size_t length, int round, int id) {
    FillPayload(payload, length, round, id);
    snprintf(reinterpret_cast<char *>(payload), length,
        "E2HOSTUDP|pid=%d|round=%02d|id=%04d|", getpid(), round, id);
}

bool RunHostUdp(int round, const char *address, HostResult *result, char *error, size_t capacity) {
    int fd = socket(AF_INET, SOCK_DGRAM | SOCK_CLOEXEC | SOCK_NONBLOCK, 0);
    if (fd < 0) {
        SetError(error, capacity, "host UDP socket", fd);
        return false;
    }
    sockaddr_in localAddress = {};
    localAddress.sin_family = AF_INET;
    localAddress.sin_addr.s_addr = htonl(INADDR_ANY);
    localAddress.sin_port = 0;
    if (bind(fd, reinterpret_cast<sockaddr *>(&localAddress), sizeof(localAddress)) != 0) {
        SetError(error, capacity, "host UDP client bind", -1);
        CloseFd(&fd);
        return false;
    }
    sockaddr_in destination = {};
    destination.sin_family = AF_INET;
    destination.sin_port = htons(kHostUdpPort);
    if (inet_pton(AF_INET, address, &destination.sin_addr) != 1) {
        SetMessage(error, capacity, "invalid selected host address");
        CloseFd(&fd);
        return false;
    }

    result->udpAggregate = 2166136261U;
    for (int slot = 1; slot <= kHostUdpDatagrams; ++slot) {
        const int id = round * 100 + slot;
        uint8_t sent[kHostUdpPayloadLength] = {};
        uint8_t received[kHostUdpPayloadLength + 1] = {};
        FillHostUdpPayload(sent, sizeof(sent), round, id);
        const uint32_t hash = Fnv1a(sent, sizeof(sent));
        if (!SendDatagram(fd, destination, sent, sizeof(sent), &result->udpPollEvents, error, capacity) ||
            !ReceiveDatagram(fd, destination, received, sizeof(received), sizeof(sent),
                &result->udpPollEvents, error, capacity) ||
            memcmp(sent, received, sizeof(sent)) != 0 || Fnv1a(received, sizeof(sent)) != hash) {
            if (error[0] == '\0') {
                SetMessage(error, capacity, "host UDP echo/hash mismatch");
            }
            CloseFd(&fd);
            return false;
        }
        result->udpAggregate = MixHash(result->udpAggregate, hash, static_cast<uint32_t>(id));
        ++result->udpDatagrams;
        OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
            "E2_HOST_UDP_DATAGRAM|round=%{public}d|address=%{public}s|port=%{public}u|id=%{public}d|bytes=%{public}zu|hash=%{public}u|valid=true",
            round, address, kHostUdpPort, id, sizeof(sent), hash);
    }
    CloseFd(&fd);
    return result->udpDatagrams == kHostUdpDatagrams;
}

bool IsLoopbackAddress(const addrinfo *entry, int *ipv4Count, int *ipv6Count) {
    if (entry->ai_family == AF_INET && entry->ai_addrlen >= sizeof(sockaddr_in)) {
        const auto *address = reinterpret_cast<const sockaddr_in *>(entry->ai_addr);
        if ((ntohl(address->sin_addr.s_addr) & 0xff000000U) == 0x7f000000U) {
            ++(*ipv4Count);
            return true;
        }
        return false;
    }
    if (entry->ai_family == AF_INET6 && entry->ai_addrlen >= sizeof(sockaddr_in6)) {
        const auto *address = reinterpret_cast<const sockaddr_in6 *>(entry->ai_addr);
        if (IN6_IS_ADDR_LOOPBACK(&address->sin6_addr)) {
            ++(*ipv6Count);
            return true;
        }
        return false;
    }
    return false;
}

bool RunDns(DnsResult *result, char *error, size_t capacity) {
    addrinfo hints = {};
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;
    addrinfo *addresses = nullptr;
    const int localhostStatus = getaddrinfo("localhost", "80", &hints, &addresses);
    if (localhostStatus != 0 || addresses == nullptr) {
        snprintf(error, capacity, "getaddrinfo localhost failed code=%d message=%s",
            localhostStatus, gai_strerror(localhostStatus));
        if (addresses != nullptr) {
            freeaddrinfo(addresses);
        }
        return false;
    }
    bool allLoopback = true;
    for (const addrinfo *entry = addresses; entry != nullptr; entry = entry->ai_next) {
        ++result->localhostCount;
        if (!IsLoopbackAddress(entry, &result->ipv4Count, &result->ipv6Count)) {
            allLoopback = false;
        }
    }
    freeaddrinfo(addresses);
    if (!allLoopback || result->localhostCount <= 0 || result->ipv4Count + result->ipv6Count != result->localhostCount) {
        SetMessage(error, capacity, "localhost getaddrinfo returned a non-loopback address");
        return false;
    }

    addrinfo invalidHints = {};
    invalidHints.ai_family = AF_UNSPEC;
    invalidHints.ai_socktype = SOCK_STREAM;
    invalidHints.ai_flags = AI_NUMERICHOST;
    addrinfo *invalidAddresses = nullptr;
    result->invalidCode = getaddrinfo("e2-reserved.invalid", nullptr, &invalidHints, &invalidAddresses);
    snprintf(result->invalidMessage, sizeof(result->invalidMessage), "%s", gai_strerror(result->invalidCode));
    if (invalidAddresses != nullptr) {
        freeaddrinfo(invalidAddresses);
        invalidAddresses = nullptr;
    }
    if (result->invalidCode != EAI_NONAME) {
        snprintf(error, capacity, "reserved invalid name did not propagate EAI_NONAME actual=%d",
            result->invalidCode);
        return false;
    }
    return true;
}

napi_value ThrowError(napi_env env, const char *message) {
    napi_throw_error(env, nullptr, message);
    return nullptr;
}

bool SetNamedInt32(napi_env env, napi_value object, const char *name, int32_t value) {
    napi_value property = nullptr;
    return napi_create_int32(env, value, &property) == napi_ok &&
        napi_set_named_property(env, object, name, property) == napi_ok;
}

bool SetNamedUint32(napi_env env, napi_value object, const char *name, uint32_t value) {
    napi_value property = nullptr;
    return napi_create_uint32(env, value, &property) == napi_ok &&
        napi_set_named_property(env, object, name, property) == napi_ok;
}

bool SetNamedString(napi_env env, napi_value object, const char *name, const char *value) {
    napi_value property = nullptr;
    return napi_create_string_utf8(env, value, NAPI_AUTO_LENGTH, &property) == napi_ok &&
        napi_set_named_property(env, object, name, property) == napi_ok;
}

napi_value MakeString(napi_env env, const char *value) {
    napi_value result = nullptr;
    if (napi_create_string_utf8(env, value, NAPI_AUTO_LENGTH, &result) != napi_ok) {
        return nullptr;
    }
    return result;
}

napi_value Version(napi_env env, napi_callback_info info) {
    (void)info;
    return MakeString(env, kProbeVersion);
}

napi_value RunNetworkRound(napi_env env, napi_callback_info info) {
    size_t argc = 1;
    napi_value argv[1] = {nullptr};
    int32_t round = 0;
    if (napi_get_cb_info(env, info, &argc, argv, nullptr, nullptr) != napi_ok || argc != 1 ||
        napi_get_value_int32(env, argv[0], &round) != napi_ok || round < 1 || round > 10) {
        return ThrowError(env, "runNetworkRound requires round 1..10");
    }

    char error[256] = {};
    TcpLoopResult tcp = {};
    UdpLoopResult udp = {};
    RouteCandidates candidates = {};
    HostResult host = {};
    DnsResult dns = {};
    if (!RunTcpLoopback(round, &tcp, error, sizeof(error)) ||
        !RunUdpLoopback(round, &udp, error, sizeof(error)) ||
        !DiscoverRouteCandidates(&candidates, error, sizeof(error))) {
        OH_LOG_Print(LOG_APP, LOG_ERROR, kLogDomain, kLogTag,
            "E2_NATIVE_ERROR|round=%{public}d|detail=%{public}s", round, error);
        return ThrowError(env, error);
    }
    snprintf(host.route, sizeof(host.route), "%s", candidates.route);
    if (!RunHostTcp(round, candidates, &host, error, sizeof(error)) ||
        !RunHostUdp(round, host.address, &host, error, sizeof(error)) ||
        !RunDns(&dns, error, sizeof(error))) {
        OH_LOG_Print(LOG_APP, LOG_ERROR, kLogDomain, kLogTag,
            "E2_NATIVE_ERROR|round=%{public}d|detail=%{public}s", round, error);
        return ThrowError(env, error);
    }

    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
        "E2_TCP_LOOPBACK|verdict=PASS|round=%{public}d|listenerGeneration=%{public}d|port=%{public}d|bytes=%{public}d|sentHash=%{public}u|echoHash=%{public}u|clientWriteCalls=%{public}d|serverPartialReads=%{public}d|serverWriteCalls=%{public}d|clientPartialReads=%{public}d|epollEvents=%{public}d",
        round, round, tcp.listenerPort, tcp.bytes, tcp.sentHash, tcp.echoHash, tcp.clientWriteCalls,
        tcp.serverReadCalls, tcp.serverWriteCalls, tcp.clientReadCalls, tcp.epollEvents);
    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
        "E2_UDP_LOOPBACK|verdict=PASS|round=%{public}d|c2s=%{public}d|s2c=%{public}d|clientAggregate=%{public}u|serverAggregate=%{public}u|pollEvents=%{public}d",
        round, udp.clientToServer, udp.serverToClient, udp.clientAggregate, udp.serverAggregate, udp.pollEvents);
    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
        "E2_HOST_ROUTE|verdict=PASS|round=%{public}d|source=%{public}s|route=%{public}s|selected=%{public}s|candidateAttempts=%{public}d",
        round, candidates.source, host.route, host.address, host.candidateAttempts);
    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
        "E2_HOST_TCP|verdict=PASS|round=%{public}d|address=%{public}s|port=%{public}u|bytes=%{public}d|echoHash=%{public}u|writeCalls=%{public}d|partialReads=%{public}d|pollEvents=%{public}d",
        round, host.address, kHostTcpPort, host.tcpBytes, host.tcpHash, host.tcpWriteCalls,
        host.tcpReadCalls, host.tcpPollEvents);
    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
        "E2_HOST_UDP|verdict=PASS|round=%{public}d|address=%{public}s|port=%{public}u|datagrams=%{public}d|aggregate=%{public}u|pollEvents=%{public}d",
        round, host.address, kHostUdpPort, host.udpDatagrams, host.udpAggregate, host.udpPollEvents);
    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
        "E2_DNS|verdict=PASS|round=%{public}d|localhostCount=%{public}d|ipv4=%{public}d|ipv6=%{public}d|allLoopback=true|invalidName=e2-reserved.invalid|invalidFlags=AI_NUMERICHOST|invalidCode=%{public}d|invalidMessage=%{public}s|publicDnsUsed=false",
        round, dns.localhostCount, dns.ipv4Count, dns.ipv6Count, dns.invalidCode, dns.invalidMessage);
    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
        "E2_EVENT_PATHS|verdict=PASS|round=%{public}d|epollEvents=%{public}d|pollTimeoutMs=%{public}d|peerCloseEOF=%{public}d|listenerCloseErrno=%{public}d|refusedErrno=%{public}d",
        round, tcp.epollEvents, tcp.timeoutElapsedMs, tcp.peerCloseEvents,
        tcp.listenerCloseErrno, tcp.refusedErrno);

    napi_value value = nullptr;
    if (napi_create_object(env, &value) != napi_ok ||
        !SetNamedInt32(env, value, "pid", static_cast<int32_t>(getpid())) ||
        !SetNamedInt32(env, value, "tid", CurrentTid()) ||
        !SetNamedInt32(env, value, "round", round) ||
        !SetNamedInt32(env, value, "tcpLoopBytes", tcp.bytes) ||
        !SetNamedUint32(env, value, "tcpLoopHash", tcp.sentHash) ||
        !SetNamedInt32(env, value, "tcpLoopEpollEvents", tcp.epollEvents) ||
        !SetNamedInt32(env, value, "tcpLoopTimeoutMs", tcp.timeoutElapsedMs) ||
        !SetNamedInt32(env, value, "tcpLoopPeerClose", tcp.peerCloseEvents) ||
        !SetNamedInt32(env, value, "tcpLoopRefusedErrno", tcp.refusedErrno) ||
        !SetNamedInt32(env, value, "tcpLoopListenerCloseErrno", tcp.listenerCloseErrno) ||
        !SetNamedInt32(env, value, "tcpLoopServerReads", tcp.serverReadCalls) ||
        !SetNamedInt32(env, value, "tcpLoopClientReads", tcp.clientReadCalls) ||
        !SetNamedInt32(env, value, "udpLoopClientToServer", udp.clientToServer) ||
        !SetNamedInt32(env, value, "udpLoopServerToClient", udp.serverToClient) ||
        !SetNamedUint32(env, value, "udpLoopClientAggregate", udp.clientAggregate) ||
        !SetNamedUint32(env, value, "udpLoopServerAggregate", udp.serverAggregate) ||
        !SetNamedString(env, value, "hostRoute", host.route) ||
        !SetNamedString(env, value, "hostAddress", host.address) ||
        !SetNamedInt32(env, value, "hostTcpBytes", host.tcpBytes) ||
        !SetNamedUint32(env, value, "hostTcpHash", host.tcpHash) ||
        !SetNamedInt32(env, value, "hostUdpDatagrams", host.udpDatagrams) ||
        !SetNamedUint32(env, value, "hostUdpAggregate", host.udpAggregate) ||
        !SetNamedInt32(env, value, "localhostCount", dns.localhostCount) ||
        !SetNamedInt32(env, value, "invalidDnsCode", dns.invalidCode)) {
        return ThrowError(env, "failed to create E2 network result");
    }
    return value;
}

napi_value Init(napi_env env, napi_value exports) {
    napi_property_descriptor properties[] = {
        {"version", nullptr, Version, nullptr, nullptr, nullptr, napi_default, nullptr},
        {"runNetworkRound", nullptr, RunNetworkRound, nullptr, nullptr, nullptr, napi_default, nullptr},
    };
    if (napi_define_properties(env, exports, sizeof(properties) / sizeof(properties[0]), properties) != napi_ok) {
        return nullptr;
    }
    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
        "Node-API E2 C network module initialized version=%{public}s", kProbeVersion);
    return exports;
}

napi_module g_e2NetworkModule = {
    1,
    0,
    nullptr,
    Init,
    "e2network",
    nullptr,
    {0},
};
} // namespace

extern "C" __attribute__((constructor, visibility("default"))) void RegisterE2NetworkModule() {
    napi_module_register(&g_e2NetworkModule);
}
