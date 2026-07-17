#define _GNU_SOURCE
#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/types.h>
#include <time.h>
#include <unistd.h>

#define HOST_TCP_PORT 39021
#define HOST_UDP_PORT 39022
#define EXPECTED_TCP_CONNECTIONS 30
#define EXPECTED_UDP_DATAGRAMS 90
#define TCP_PAYLOAD_LENGTH 2048
#define UDP_PAYLOAD_LENGTH 224
#define MAX_PIDS 3
#define IO_TIMEOUT_MS 5000
#define OVERALL_TIMEOUT_SECONDS 300

struct PidCounts {
    int pid;
    int tcp;
    int udp;
};

static volatile sig_atomic_t interrupted = 0;

static void handle_signal(int signal_number) {
    (void)signal_number;
    interrupted = 1;
}

static uint32_t fnv1a(const uint8_t *data, size_t length) {
    uint32_t hash = 2166136261U;
    for (size_t index = 0; index < length; ++index) {
        hash ^= data[index];
        hash *= 16777619U;
    }
    return hash;
}

static int wait_fd(int fd, short events) {
    struct pollfd descriptor = {fd, events, 0};
    int status;
    do {
        status = poll(&descriptor, 1, IO_TIMEOUT_MS);
    } while (status < 0 && errno == EINTR && !interrupted);
    if (status != 1 || (descriptor.revents & (POLLNVAL | POLLERR)) != 0) {
        fprintf(stderr, "HOST_SERVER_ERROR|operation=poll|fd=%d|status=%d|revents=%d|errno=%d\n",
            fd, status, descriptor.revents, errno);
        return -1;
    }
    return 0;
}

static int read_exact(int fd, uint8_t *data, size_t length, int *calls) {
    size_t offset = 0;
    while (offset < length) {
        if (wait_fd(fd, POLLIN) != 0) {
            return -1;
        }
        size_t requested = length - offset;
        if (requested > 113) {
            requested = 113;
        }
        ssize_t received = recv(fd, data + offset, requested, 0);
        if (received < 0 && (errno == EINTR || errno == EAGAIN || errno == EWOULDBLOCK)) {
            continue;
        }
        if (received <= 0) {
            fprintf(stderr, "HOST_SERVER_ERROR|operation=recv|result=%zd|errno=%d\n", received, errno);
            return -1;
        }
        offset += (size_t)received;
        ++(*calls);
    }
    return 0;
}

static int write_chunked(int fd, const uint8_t *data, size_t length, int *calls) {
    const size_t chunks[] = {67, 251, 31, 509, 97};
    size_t offset = 0;
    size_t chunk_index = 0;
    while (offset < length) {
        if (wait_fd(fd, POLLOUT) != 0) {
            return -1;
        }
        size_t requested = chunks[chunk_index % (sizeof(chunks) / sizeof(chunks[0]))];
        if (requested > length - offset) {
            requested = length - offset;
        }
        ssize_t written = send(fd, data + offset, requested, MSG_NOSIGNAL);
        if (written < 0 && (errno == EINTR || errno == EAGAIN || errno == EWOULDBLOCK)) {
            continue;
        }
        if (written <= 0) {
            fprintf(stderr, "HOST_SERVER_ERROR|operation=send|result=%zd|errno=%d\n", written, errno);
            return -1;
        }
        offset += (size_t)written;
        ++(*calls);
        ++chunk_index;
    }
    return 0;
}

static int set_nonblocking(int fd) {
    int flags = fcntl(fd, F_GETFL, 0);
    if (flags < 0 || fcntl(fd, F_SETFL, flags | O_NONBLOCK) != 0) {
        return -1;
    }
    return 0;
}

static int make_tcp_listener(void) {
    int fd = socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC, 0);
    if (fd < 0) {
        return -1;
    }
    int enabled = 1;
    if (setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &enabled, sizeof(enabled)) != 0 ||
        set_nonblocking(fd) != 0) {
        close(fd);
        return -1;
    }
    struct sockaddr_in address = {0};
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    address.sin_port = htons(HOST_TCP_PORT);
    if (bind(fd, (struct sockaddr *)&address, sizeof(address)) != 0 || listen(fd, 8) != 0) {
        close(fd);
        return -1;
    }
    return fd;
}

static int make_udp_socket(void) {
    int fd = socket(AF_INET, SOCK_DGRAM | SOCK_CLOEXEC, 0);
    if (fd < 0) {
        return -1;
    }
    struct sockaddr_in address = {0};
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    address.sin_port = htons(HOST_UDP_PORT);
    if (bind(fd, (struct sockaddr *)&address, sizeof(address)) != 0 || set_nonblocking(fd) != 0) {
        close(fd);
        return -1;
    }
    return fd;
}

static int find_pid(struct PidCounts *counts, int *count, int pid) {
    for (int index = 0; index < *count; ++index) {
        if (counts[index].pid == pid) {
            return index;
        }
    }
    if (*count >= MAX_PIDS) {
        return -1;
    }
    counts[*count].pid = pid;
    ++(*count);
    return *count - 1;
}

static int parse_header(const uint8_t *payload, const char *prefix, int *pid, int *round, int *id) {
    char header[128] = {0};
    memcpy(header, payload, sizeof(header) - 1);
    char format[64] = {0};
    snprintf(format, sizeof(format), "%s|pid=%%d|round=%%d|id=%%d|", prefix);
    if (sscanf(header, format, pid, round, id) != 3 || *pid <= 0 || *round < 1 || *round > 10) {
        return -1;
    }
    return 0;
}

static int handle_tcp(int listener, struct PidCounts *counts, int *pid_count, int sequence) {
    struct sockaddr_in peer = {0};
    socklen_t peer_length = sizeof(peer);
    int client = accept4(listener, (struct sockaddr *)&peer, &peer_length, SOCK_CLOEXEC | SOCK_NONBLOCK);
    if (client < 0) {
        fprintf(stderr, "HOST_SERVER_ERROR|operation=accept4|errno=%d\n", errno);
        return -1;
    }
    uint8_t payload[TCP_PAYLOAD_LENGTH] = {0};
    int read_calls = 0;
    int write_calls = 0;
    int pid = 0;
    int round = 0;
    int id = 0;
    int status = read_exact(client, payload, sizeof(payload), &read_calls);
    if (status == 0) {
        status = parse_header(payload, "E2HOSTTCP", &pid, &round, &id);
    }
    if (status == 0 && id != round * 100 + 7) {
        status = -1;
    }
    if (status == 0) {
        status = write_chunked(client, payload, sizeof(payload), &write_calls);
    }
    int close_status = close(client);
    if (status != 0 || close_status != 0) {
        fprintf(stderr,
            "HOST_SERVER_ERROR|operation=tcp_exchange|sequence=%d|pid=%d|round=%d|id=%d|close=%d|errno=%d\n",
            sequence, pid, round, id, close_status, errno);
        return -1;
    }
    int pid_index = find_pid(counts, pid_count, pid);
    if (pid_index < 0) {
        fprintf(stderr, "HOST_SERVER_ERROR|operation=tcp_pid_set|pid=%d\n", pid);
        return -1;
    }
    ++counts[pid_index].tcp;
    char peer_address[INET_ADDRSTRLEN] = {0};
    inet_ntop(AF_INET, &peer.sin_addr, peer_address, sizeof(peer_address));
    printf("HOST_TCP|sequence=%d|pid=%d|round=%d|id=%d|peer=%s|bytes=%zu|hash=%u|readCalls=%d|writeCalls=%d|echo=true\n",
        sequence, pid, round, id, peer_address, sizeof(payload), fnv1a(payload, sizeof(payload)),
        read_calls, write_calls);
    fflush(stdout);
    return 0;
}

static int handle_udp(int fd, struct PidCounts *counts, int *pid_count, int sequence) {
    uint8_t payload[UDP_PAYLOAD_LENGTH + 1] = {0};
    struct sockaddr_in peer = {0};
    socklen_t peer_length = sizeof(peer);
    ssize_t received = recvfrom(fd, payload, sizeof(payload), 0, (struct sockaddr *)&peer, &peer_length);
    if (received != UDP_PAYLOAD_LENGTH) {
        fprintf(stderr, "HOST_SERVER_ERROR|operation=udp_recvfrom|sequence=%d|bytes=%zd|errno=%d\n",
            sequence, received, errno);
        return -1;
    }
    int pid = 0;
    int round = 0;
    int id = 0;
    if (parse_header(payload, "E2HOSTUDP", &pid, &round, &id) != 0 ||
        id < round * 100 + 1 || id > round * 100 + 3) {
        fprintf(stderr,
            "HOST_SERVER_ERROR|operation=udp_header|sequence=%d|pid=%d|round=%d|id=%d\n",
            sequence, pid, round, id);
        return -1;
    }
    ssize_t sent = sendto(fd, payload, (size_t)received, 0, (struct sockaddr *)&peer, peer_length);
    if (sent != received) {
        fprintf(stderr, "HOST_SERVER_ERROR|operation=udp_sendto|sequence=%d|bytes=%zd|errno=%d\n",
            sequence, sent, errno);
        return -1;
    }
    int pid_index = find_pid(counts, pid_count, pid);
    if (pid_index < 0) {
        fprintf(stderr, "HOST_SERVER_ERROR|operation=udp_pid_set|pid=%d\n", pid);
        return -1;
    }
    ++counts[pid_index].udp;
    char peer_address[INET_ADDRSTRLEN] = {0};
    inet_ntop(AF_INET, &peer.sin_addr, peer_address, sizeof(peer_address));
    printf("HOST_UDP|sequence=%d|pid=%d|round=%d|id=%d|peer=%s|bytes=%zd|hash=%u|echo=true\n",
        sequence, pid, round, id, peer_address, received, fnv1a(payload, (size_t)received));
    fflush(stdout);
    return 0;
}

int main(void) {
    signal(SIGINT, handle_signal);
    signal(SIGTERM, handle_signal);
    int listener = make_tcp_listener();
    int udp = make_udp_socket();
    if (listener < 0 || udp < 0) {
        fprintf(stderr,
            "HOST_SERVER_ERROR|operation=bind|tcpFd=%d|udpFd=%d|tcpAddress=127.0.0.1|tcpPort=%d|udpPort=%d|errno=%d\n",
            listener, udp, HOST_TCP_PORT, HOST_UDP_PORT, errno);
        if (listener >= 0) {
            close(listener);
        }
        if (udp >= 0) {
            close(udp);
        }
        return 1;
    }

    printf("HOST_SERVER_READY|pid=%d|tcp=127.0.0.1:%d|udp=127.0.0.1:%d|expectedTcp=%d|expectedUdp=%d|publicBind=false\n",
        getpid(), HOST_TCP_PORT, HOST_UDP_PORT, EXPECTED_TCP_CONNECTIONS, EXPECTED_UDP_DATAGRAMS);
    fflush(stdout);

    struct PidCounts counts[MAX_PIDS] = {0};
    int pid_count = 0;
    int tcp_count = 0;
    int udp_count = 0;
    time_t started = time(NULL);
    int verdict = 0;
    while (!interrupted &&
        (tcp_count < EXPECTED_TCP_CONNECTIONS || udp_count < EXPECTED_UDP_DATAGRAMS)) {
        if (time(NULL) - started > OVERALL_TIMEOUT_SECONDS) {
            fprintf(stderr, "HOST_SERVER_ERROR|operation=overall_timeout|tcp=%d|udp=%d\n",
                tcp_count, udp_count);
            verdict = 1;
            break;
        }
        struct pollfd descriptors[2] = {
            {listener, tcp_count < EXPECTED_TCP_CONNECTIONS ? POLLIN : 0, 0},
            {udp, udp_count < EXPECTED_UDP_DATAGRAMS ? POLLIN : 0, 0},
        };
        int status;
        do {
            status = poll(descriptors, 2, IO_TIMEOUT_MS);
        } while (status < 0 && errno == EINTR && !interrupted);
        if (status < 0) {
            fprintf(stderr, "HOST_SERVER_ERROR|operation=main_poll|status=%d|errno=%d\n", status, errno);
            verdict = 1;
            break;
        }
        if (status == 0) {
            printf("HOST_SERVER_WAIT|tcp=%d/%d|udp=%d/%d\n",
                tcp_count, EXPECTED_TCP_CONNECTIONS, udp_count, EXPECTED_UDP_DATAGRAMS);
            fflush(stdout);
            continue;
        }
        if ((descriptors[0].revents & POLLIN) != 0) {
            if (handle_tcp(listener, counts, &pid_count, tcp_count + 1) != 0) {
                verdict = 1;
                break;
            }
            ++tcp_count;
        }
        if ((descriptors[1].revents & POLLIN) != 0) {
            if (handle_udp(udp, counts, &pid_count, udp_count + 1) != 0) {
                verdict = 1;
                break;
            }
            ++udp_count;
        }
    }

    if (interrupted) {
        fprintf(stderr, "HOST_SERVER_ERROR|operation=signal|tcp=%d|udp=%d\n", tcp_count, udp_count);
        verdict = 1;
    }
    if (tcp_count != EXPECTED_TCP_CONNECTIONS || udp_count != EXPECTED_UDP_DATAGRAMS ||
        pid_count != MAX_PIDS) {
        fprintf(stderr, "HOST_SERVER_ERROR|operation=final_counts|tcp=%d|udp=%d|pids=%d\n",
            tcp_count, udp_count, pid_count);
        verdict = 1;
    }
    for (int index = 0; index < pid_count; ++index) {
        printf("HOST_PID_SUMMARY|pid=%d|tcp=%d|udp=%d|expectedTcp=10|expectedUdp=30|verdict=%s\n",
            counts[index].pid, counts[index].tcp, counts[index].udp,
            counts[index].tcp == 10 && counts[index].udp == 30 ? "PASS" : "FAIL");
        if (counts[index].tcp != 10 || counts[index].udp != 30) {
            verdict = 1;
        }
    }
    printf("HOST_SERVER_RESULT|verdict=%s|tcp=%d|udp=%d|pids=%d\n",
        verdict == 0 ? "PASS" : "FAIL", tcp_count, udp_count, pid_count);
    fflush(stdout);

    close(udp);
    close(listener);
    return verdict;
}
