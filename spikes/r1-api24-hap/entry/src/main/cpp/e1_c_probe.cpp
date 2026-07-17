#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <hilog/log.h>
#include <napi/native_api.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/syscall.h>
#include <sys/types.h>
#include <unistd.h>

namespace {
constexpr unsigned int kLogDomain = 0x2900;
constexpr const char *kLogTag = "R1Api24Probe";
constexpr const char *kProbeVersion = "e1-c-api24-probe/0.0.1";
constexpr int kCallbacksPerRound = 100;

struct CallbackData {
    int round;
    int sequence;
    int producerTid;
    uint32_t payloadHash;
    char payload[64];
};

struct WorkerState {
    pthread_t thread;
    int round;
    int producerTid;
    int queued;
    int status;
    bool active;
};

napi_threadsafe_function g_threadsafeFunction = nullptr;
WorkerState g_worker = {};

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

bool ReadInt32Argument(napi_env env, napi_value value, int32_t *result) {
    return napi_get_value_int32(env, value, result) == napi_ok;
}

int CountProcEntries(const char *path) {
    DIR *directory = opendir(path);
    if (directory == nullptr) {
        return -errno;
    }

    int count = 0;
    errno = 0;
    while (dirent *entry = readdir(directory)) {
        if (strcmp(entry->d_name, ".") != 0 && strcmp(entry->d_name, "..") != 0) {
            ++count;
        }
    }
    const int readError = errno;
    if (closedir(directory) != 0 && readError == 0) {
        return -errno;
    }
    return readError == 0 ? count : -readError;
}

napi_value MakeResourceSnapshot(napi_env env) {
    const int fdCount = CountProcEntries("/proc/self/fd");
    const int threadCount = CountProcEntries("/proc/self/task");
    if (fdCount < 0 || threadCount < 0) {
        return ThrowError(env, "failed to read /proc/self resource snapshot");
    }

    napi_value result = nullptr;
    if (napi_create_object(env, &result) != napi_ok ||
        !SetNamedInt32(env, result, "pid", static_cast<int32_t>(getpid())) ||
        !SetNamedInt32(env, result, "tid", CurrentTid()) ||
        !SetNamedInt32(env, result, "fdCount", fdCount) ||
        !SetNamedInt32(env, result, "threadCount", threadCount)) {
        return ThrowError(env, "failed to create resource snapshot");
    }
    return result;
}

napi_value Ping(napi_env env, napi_callback_info info) {
    (void)info;
    return MakeString(env, "pong");
}

napi_value Version(napi_env env, napi_callback_info info) {
    (void)info;
    return MakeString(env, kProbeVersion);
}

napi_value ResourceSnapshot(napi_env env, napi_callback_info info) {
    (void)info;
    return MakeResourceSnapshot(env);
}

napi_value HashBuffer(napi_env env, napi_callback_info info) {
    size_t argc = 1;
    napi_value argv[1] = {nullptr};
    if (napi_get_cb_info(env, info, &argc, argv, nullptr, nullptr) != napi_ok || argc != 1) {
        return ThrowError(env, "hashBuffer requires one Uint8Array");
    }

    napi_typedarray_type type = napi_uint8_array;
    size_t length = 0;
    void *data = nullptr;
    napi_value arrayBuffer = nullptr;
    size_t byteOffset = 0;
    if (napi_get_typedarray_info(env, argv[0], &type, &length, &data, &arrayBuffer, &byteOffset) != napi_ok ||
        type != napi_uint8_array || (length > 0 && data == nullptr)) {
        return ThrowError(env, "hashBuffer argument is not a valid Uint8Array");
    }

    const auto *bytes = static_cast<const uint8_t *>(data);
    const uint32_t hash = Fnv1a(bytes, length);
    const int first = length == 0 ? -1 : bytes[0];
    const int last = length == 0 ? -1 : bytes[length - 1];

    napi_value result = nullptr;
    if (napi_create_object(env, &result) != napi_ok ||
        !SetNamedUint32(env, result, "hash", hash) ||
        !SetNamedInt32(env, result, "length", static_cast<int32_t>(length)) ||
        !SetNamedInt32(env, result, "first", first) ||
        !SetNamedInt32(env, result, "last", last) ||
        !SetNamedInt32(env, result, "nativeTid", CurrentTid())) {
        return ThrowError(env, "failed to create hashBuffer result");
    }
    return result;
}

napi_value CreateFdPair(napi_env env, napi_callback_info info) {
    (void)info;
    int pair[2] = {-1, -1};
    if (socketpair(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0, pair) != 0) {
        return ThrowError(env, "socketpair failed");
    }

    napi_value result = nullptr;
    if (napi_create_object(env, &result) != napi_ok ||
        !SetNamedInt32(env, result, "fdA", pair[0]) ||
        !SetNamedInt32(env, result, "fdB", pair[1]) ||
        !SetNamedInt32(env, result, "creatorTid", CurrentTid())) {
        close(pair[0]);
        close(pair[1]);
        return ThrowError(env, "failed to create fd pair result");
    }

    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
        "E1_FD_CREATED|fdA=%{public}d|fdB=%{public}d|creatorTid=%{public}d|owner=ArkTS",
        pair[0], pair[1], CurrentTid());
    return result;
}

bool WriteAll(int fd, const char *data, size_t length, int *written) {
    size_t offset = 0;
    while (offset < length) {
        const ssize_t result = write(fd, data + offset, length - offset);
        if (result < 0 && errno == EINTR) {
            continue;
        }
        if (result <= 0) {
            return false;
        }
        offset += static_cast<size_t>(result);
    }
    *written = static_cast<int>(offset);
    return true;
}

bool ReadAll(int fd, char *data, size_t length, int *readBytes) {
    size_t offset = 0;
    while (offset < length) {
        const ssize_t result = read(fd, data + offset, length - offset);
        if (result < 0 && errno == EINTR) {
            continue;
        }
        if (result <= 0) {
            return false;
        }
        offset += static_cast<size_t>(result);
    }
    *readBytes = static_cast<int>(offset);
    return true;
}

napi_value TransferFdOwnership(napi_env env, napi_callback_info info) {
    size_t argc = 3;
    napi_value argv[3] = {nullptr, nullptr, nullptr};
    if (napi_get_cb_info(env, info, &argc, argv, nullptr, nullptr) != napi_ok || argc != 3) {
        return ThrowError(env, "transferFdOwnership requires fdA, fdB, and round");
    }

    int32_t fdA = -1;
    int32_t fdB = -1;
    int32_t round = -1;
    if (!ReadInt32Argument(env, argv[0], &fdA) || !ReadInt32Argument(env, argv[1], &fdB) ||
        !ReadInt32Argument(env, argv[2], &round) || fdA < 0 || fdB < 0 || fdA == fdB || round < 1) {
        return ThrowError(env, "invalid fd ownership transfer arguments");
    }
    if (fcntl(fdA, F_GETFD) < 0 || fcntl(fdB, F_GETFD) < 0) {
        return ThrowError(env, "fd ownership transfer received a closed descriptor");
    }

    const int duplicateFd = dup(fdA);
    if (duplicateFd < 0) {
        close(fdA);
        close(fdB);
        return ThrowError(env, "dup failed after ownership transfer");
    }

    const int closeOriginalA = close(fdA);
    char payload[64] = {};
    const int payloadLength = snprintf(payload, sizeof(payload), "fd-round-%02d-sentinel-%02x", round,
        (round * 37) & 0xff);
    char received[64] = {};
    int written = 0;
    int readBytes = 0;
    const bool ioOk = payloadLength > 0 && static_cast<size_t>(payloadLength) < sizeof(payload) &&
        WriteAll(duplicateFd, payload, static_cast<size_t>(payloadLength), &written) &&
        ReadAll(fdB, received, static_cast<size_t>(payloadLength), &readBytes) &&
        memcmp(payload, received, static_cast<size_t>(payloadLength)) == 0;

    const int closeDuplicate = close(duplicateFd);
    const int closeOriginalB = close(fdB);
    errno = 0;
    const int duplicateCloseA = close(fdA);
    const int duplicateCloseErrnoA = errno;
    errno = 0;
    const int duplicateCloseB = close(fdB);
    const int duplicateCloseErrnoB = errno;

    if (closeOriginalA != 0 || !ioOk || closeDuplicate != 0 || closeOriginalB != 0 ||
        duplicateCloseA != -1 || duplicateCloseErrnoA != EBADF ||
        duplicateCloseB != -1 || duplicateCloseErrnoB != EBADF) {
        return ThrowError(env, "fd ownership, I/O, close, or duplicate-close check failed");
    }

    const uint32_t payloadHash = Fnv1a(reinterpret_cast<const uint8_t *>(payload),
        static_cast<size_t>(payloadLength));
    napi_value result = nullptr;
    if (napi_create_object(env, &result) != napi_ok ||
        !SetNamedInt32(env, result, "fdA", fdA) ||
        !SetNamedInt32(env, result, "fdB", fdB) ||
        !SetNamedInt32(env, result, "duplicateFd", duplicateFd) ||
        !SetNamedInt32(env, result, "written", written) ||
        !SetNamedInt32(env, result, "read", readBytes) ||
        !SetNamedInt32(env, result, "closeOriginalA", closeOriginalA) ||
        !SetNamedInt32(env, result, "closeDuplicate", closeDuplicate) ||
        !SetNamedInt32(env, result, "closeOriginalB", closeOriginalB) ||
        !SetNamedInt32(env, result, "duplicateCloseA", duplicateCloseA) ||
        !SetNamedInt32(env, result, "duplicateCloseErrnoA", duplicateCloseErrnoA) ||
        !SetNamedInt32(env, result, "duplicateCloseB", duplicateCloseB) ||
        !SetNamedInt32(env, result, "duplicateCloseErrnoB", duplicateCloseErrnoB) ||
        !SetNamedUint32(env, result, "payloadHash", payloadHash) ||
        !SetNamedString(env, result, "payload", payload) ||
        !SetNamedInt32(env, result, "nativeTid", CurrentTid())) {
        return ThrowError(env, "failed to create fd transfer result");
    }
    return result;
}

void CallArkTs(napi_env env, napi_value jsCallback, void *context, void *data) {
    (void)context;
    auto *callbackData = static_cast<CallbackData *>(data);
    if (callbackData == nullptr) {
        return;
    }
    if (env == nullptr || jsCallback == nullptr) {
        delete callbackData;
        return;
    }

    napi_value undefinedValue = nullptr;
    napi_value argv[6] = {nullptr, nullptr, nullptr, nullptr, nullptr, nullptr};
    napi_get_undefined(env, &undefinedValue);
    napi_create_int32(env, callbackData->round, &argv[0]);
    napi_create_int32(env, callbackData->sequence, &argv[1]);
    napi_create_string_utf8(env, callbackData->payload, NAPI_AUTO_LENGTH, &argv[2]);
    napi_create_uint32(env, callbackData->payloadHash, &argv[3]);
    napi_create_int32(env, callbackData->producerTid, &argv[4]);
    napi_create_int32(env, CurrentTid(), &argv[5]);

    napi_value ignored = nullptr;
    const napi_status status = napi_call_function(env, undefinedValue, jsCallback, 6, argv, &ignored);
    if (status != napi_ok) {
        OH_LOG_Print(LOG_APP, LOG_ERROR, kLogDomain, kLogTag,
            "E1_CALLBACK_DISPATCH_ERROR|round=%{public}d|sequence=%{public}d|status=%{public}d",
            callbackData->round, callbackData->sequence, static_cast<int>(status));
    }
    delete callbackData;
}

void ThreadsafeFinalize(napi_env env, void *finalizeData, void *finalizeHint) {
    (void)env;
    (void)finalizeData;
    (void)finalizeHint;
    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
        "E1_TSFN_FINALIZED|consumerTid=%{public}d", CurrentTid());
}

void *CallbackProducer(void *argument) {
    auto *worker = static_cast<WorkerState *>(argument);
    pthread_setname_np(pthread_self(), "e1-c-callback");
    worker->producerTid = CurrentTid();
    worker->queued = 0;
    worker->status = static_cast<int>(napi_ok);
    if (worker->round == 0) {
        OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
            "E1_PTHREAD_WARMUP_START|round=0|producerTid=%{public}d", worker->producerTid);
    } else {
        OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
            "E1_PTHREAD_START|round=%{public}d|producerTid=%{public}d", worker->round, worker->producerTid);
    }

    for (int sequence = 1; sequence <= kCallbacksPerRound; ++sequence) {
        auto *callbackData = new CallbackData();
        callbackData->round = worker->round;
        callbackData->sequence = sequence;
        callbackData->producerTid = worker->producerTid;
        const int sentinel = (worker->round * 17 + sequence * 31) & 0xff;
        snprintf(callbackData->payload, sizeof(callbackData->payload), "r%02d-s%03d-x%02x",
            worker->round, sequence, sentinel);
        callbackData->payloadHash = Fnv1a(reinterpret_cast<const uint8_t *>(callbackData->payload),
            strlen(callbackData->payload));

        const napi_status status = napi_call_threadsafe_function(
            g_threadsafeFunction, callbackData, napi_tsfn_nonblocking);
        if (status != napi_ok) {
            worker->status = static_cast<int>(status);
            delete callbackData;
            break;
        }
        ++worker->queued;
    }

    const napi_status releaseStatus = napi_release_threadsafe_function(g_threadsafeFunction, napi_tsfn_release);
    if (worker->status == static_cast<int>(napi_ok) && releaseStatus != napi_ok) {
        worker->status = static_cast<int>(releaseStatus);
    }
    if (worker->round == 0) {
        OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
            "E1_PTHREAD_WARMUP_FINISH|round=0|producerTid=%{public}d|queued=%{public}d|status=%{public}d",
            worker->producerTid, worker->queued, worker->status);
    } else {
        OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
            "E1_PTHREAD_FINISH|round=%{public}d|producerTid=%{public}d|queued=%{public}d|status=%{public}d",
            worker->round, worker->producerTid, worker->queued, worker->status);
    }
    return nullptr;
}

napi_value InitializeAsync(napi_env env, napi_callback_info info) {
    if (g_threadsafeFunction != nullptr) {
        return ThrowError(env, "threadsafe function already initialized");
    }

    size_t argc = 1;
    napi_value argv[1] = {nullptr};
    if (napi_get_cb_info(env, info, &argc, argv, nullptr, nullptr) != napi_ok || argc != 1) {
        return ThrowError(env, "initializeAsync requires one ArkTS callback");
    }
    napi_valuetype valueType = napi_undefined;
    if (napi_typeof(env, argv[0], &valueType) != napi_ok || valueType != napi_function) {
        return ThrowError(env, "initializeAsync argument must be a function");
    }

    napi_value resourceName = nullptr;
    if (napi_create_string_utf8(env, "e1-c-pthread-callback", NAPI_AUTO_LENGTH, &resourceName) != napi_ok) {
        return ThrowError(env, "failed to create threadsafe resource name");
    }
    const napi_status status = napi_create_threadsafe_function(env, argv[0], nullptr, resourceName, 0, 1,
        nullptr, ThreadsafeFinalize, nullptr, CallArkTs, &g_threadsafeFunction);
    if (status != napi_ok) {
        g_threadsafeFunction = nullptr;
        return ThrowError(env, "napi_create_threadsafe_function failed");
    }
    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
        "E1_TSFN_INITIALIZED|mainTid=%{public}d|mechanism=napi_threadsafe_function", CurrentTid());
    return MakeResourceSnapshot(env);
}

napi_value StartAsyncRound(napi_env env, napi_callback_info info) {
    if (g_threadsafeFunction == nullptr) {
        return ThrowError(env, "threadsafe function is not initialized");
    }
    if (g_worker.active) {
        return ThrowError(env, "an async round is already active");
    }

    size_t argc = 1;
    napi_value argv[1] = {nullptr};
    int32_t round = 0;
    if (napi_get_cb_info(env, info, &argc, argv, nullptr, nullptr) != napi_ok || argc != 1 ||
        !ReadInt32Argument(env, argv[0], &round) || round < 0) {
        return ThrowError(env, "startAsyncRound requires a nonnegative round number");
    }

    const napi_status acquireStatus = napi_acquire_threadsafe_function(g_threadsafeFunction);
    if (acquireStatus != napi_ok) {
        return ThrowError(env, "napi_acquire_threadsafe_function failed");
    }

    g_worker.round = round;
    g_worker.producerTid = -1;
    g_worker.queued = 0;
    g_worker.status = static_cast<int>(napi_ok);
    g_worker.active = true;
    const int createStatus = pthread_create(&g_worker.thread, nullptr, CallbackProducer, &g_worker);
    if (createStatus != 0) {
        g_worker.active = false;
        napi_release_threadsafe_function(g_threadsafeFunction, napi_tsfn_release);
        return ThrowError(env, "pthread_create failed");
    }

    napi_value undefinedValue = nullptr;
    napi_get_undefined(env, &undefinedValue);
    return undefinedValue;
}

napi_value CompleteAsyncRound(napi_env env, napi_callback_info info) {
    if (!g_worker.active) {
        return ThrowError(env, "no async round is active");
    }

    size_t argc = 1;
    napi_value argv[1] = {nullptr};
    int32_t round = 0;
    if (napi_get_cb_info(env, info, &argc, argv, nullptr, nullptr) != napi_ok || argc != 1 ||
        !ReadInt32Argument(env, argv[0], &round) || round != g_worker.round) {
        return ThrowError(env, "completeAsyncRound round does not match active round");
    }

    const int joinStatus = pthread_join(g_worker.thread, nullptr);
    const int producerTid = g_worker.producerTid;
    const int queued = g_worker.queued;
    const int callbackStatus = g_worker.status;
    g_worker.active = false;

    napi_value result = nullptr;
    if (napi_create_object(env, &result) != napi_ok ||
        !SetNamedInt32(env, result, "round", round) ||
        !SetNamedInt32(env, result, "producerTid", producerTid) ||
        !SetNamedInt32(env, result, "queued", queued) ||
        !SetNamedInt32(env, result, "callbackStatus", callbackStatus) ||
        !SetNamedInt32(env, result, "joinStatus", joinStatus)) {
        return ThrowError(env, "failed to create async completion result");
    }
    return result;
}

napi_value ShutdownAsync(napi_env env, napi_callback_info info) {
    (void)info;
    if (g_threadsafeFunction == nullptr) {
        return ThrowError(env, "threadsafe function is not initialized");
    }
    if (g_worker.active) {
        return ThrowError(env, "cannot shutdown while an async round is active");
    }

    const napi_status status = napi_release_threadsafe_function(g_threadsafeFunction, napi_tsfn_release);
    if (status != napi_ok) {
        return ThrowError(env, "napi_release_threadsafe_function failed");
    }
    g_threadsafeFunction = nullptr;
    return MakeResourceSnapshot(env);
}

napi_value Init(napi_env env, napi_value exports) {
    napi_property_descriptor properties[] = {
        {"ping", nullptr, Ping, nullptr, nullptr, nullptr, napi_default, nullptr},
        {"version", nullptr, Version, nullptr, nullptr, nullptr, napi_default, nullptr},
        {"resourceSnapshot", nullptr, ResourceSnapshot, nullptr, nullptr, nullptr, napi_default, nullptr},
        {"hashBuffer", nullptr, HashBuffer, nullptr, nullptr, nullptr, napi_default, nullptr},
        {"createFdPair", nullptr, CreateFdPair, nullptr, nullptr, nullptr, napi_default, nullptr},
        {"transferFdOwnership", nullptr, TransferFdOwnership, nullptr, nullptr, nullptr, napi_default, nullptr},
        {"initializeAsync", nullptr, InitializeAsync, nullptr, nullptr, nullptr, napi_default, nullptr},
        {"startAsyncRound", nullptr, StartAsyncRound, nullptr, nullptr, nullptr, napi_default, nullptr},
        {"completeAsyncRound", nullptr, CompleteAsyncRound, nullptr, nullptr, nullptr, napi_default, nullptr},
        {"shutdownAsync", nullptr, ShutdownAsync, nullptr, nullptr, nullptr, napi_default, nullptr},
    };
    if (napi_define_properties(env, exports, sizeof(properties) / sizeof(properties[0]), properties) != napi_ok) {
        return nullptr;
    }
    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
        "Node-API E1 C module initialized version=%{public}s", kProbeVersion);
    return exports;
}

napi_module g_probeModule = {
    1,
    0,
    nullptr,
    Init,
    "probe",
    nullptr,
    {0},
};
} // namespace

extern "C" __attribute__((constructor, visibility("default"))) void RegisterProbeModule() {
    napi_module_register(&g_probeModule);
}
