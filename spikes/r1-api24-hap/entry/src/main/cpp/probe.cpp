#include <cerrno>
#include <cstring>
#include <dlfcn.h>
#include <pthread.h>
#include <string>

#include <hilog/log.h>
#include <napi/native_api.h>

namespace {
constexpr unsigned int kLogDomain = 0x2900;
constexpr const char *kLogTag = "R1Api24Probe";
constexpr const char *kProbeVersion = "r1-api24-probe/0.0.1";
constexpr const char *kInitialExecTlsRejection = "initial-exec TLS resolves to dynamic definition";
constexpr int kInitialValue = 42;
constexpr int kIterations = 100;

using GetTlsFunction = int (*)();
using SetTlsFunction = void (*)(int);
using ResetTlsFunction = void (*)();

struct LoaderObservation {
    void *address = nullptr;
    std::string error;
    int savedErrno = 0;
};

struct TlsApi {
    GetTlsFunction get = nullptr;
    SetTlsFunction set = nullptr;
    ResetTlsFunction reset = nullptr;
    std::string error;
    int savedErrno = 0;
};

struct ThreadResult {
    bool resolved = false;
    bool ok = false;
    int initialValue = -1;
    int resetValue = -1;
    int iterations = 0;
    std::string detail = "thread not started";
};

struct ThreadGate {
    pthread_mutex_t mutex;
    pthread_cond_t condition;
    void *handle = nullptr;
    bool handleReady = false;
    bool start = false;
    bool abort = false;
    int waitingWorkers = 0;
    int readyWorkers = 0;
};

struct ThreadContext {
    ThreadGate *gate = nullptr;
    const char *role = nullptr;
    int value = 0;
    ThreadResult result;
};

struct ModelResult {
    bool attempted = false;
    bool required = true;
    bool ok = false;
    bool expectedBlock = false;
    bool environmentDrift = false;
    bool preThreadCreatedBeforeDlopen = false;
    bool postThreadCreatedAfterDlopen = false;
    std::string model;
    std::string library;
    std::string functionalStatus = "NOT_RUN";
    std::string stage = "not-run";
    std::string detail = "model not run";
    std::string loaderError;
    int loaderErrno = 0;
    int mainValue = 0;
    int preThreadValue = 0;
    int postThreadValue = 0;
    int mainInitial = -1;
    int preThreadInitial = -1;
    int postThreadInitial = -1;
    int mainReset = -1;
    int preThreadReset = -1;
    int postThreadReset = -1;
    int mainIterations = 0;
    int preThreadIterations = 0;
    int postThreadIterations = 0;
};

struct SuiteResult {
    bool ok = false;
    bool ieBlocked = false;
    bool environmentDrift = false;
    bool tier1Pass = false;
    std::string verdict = "FAIL";
    std::string stopReason = "not started";
    ModelResult initialExec;
    ModelResult globalDynamic;
    ModelResult tlsDesc;
    ModelResult localDynamic;
};

void *g_retainedHandles[4] = {nullptr, nullptr, nullptr, nullptr};
bool g_suiteInvoked = false;

std::string LoaderDetail(const char *operation, const std::string &loaderError, int savedErrno) {
    std::string detail(operation);
    detail += " dlerror=";
    detail += loaderError.empty() ? "<none>" : loaderError;
    detail += "; errno=" + std::to_string(savedErrno);
    if (savedErrno != 0) {
        detail += " (";
        detail += std::strerror(savedErrno);
        detail += ")";
    }
    return detail;
}

LoaderObservation ObserveDlopen(const char *library) {
    dlerror();
    errno = 0;
    void *address = dlopen(library, RTLD_NOW | RTLD_LOCAL);
    const int savedErrno = errno;
    const char *errorPointer = dlerror();
    return {address, errorPointer == nullptr ? "" : errorPointer, savedErrno};
}

TlsApi ResolveTlsApi(void *handle) {
    TlsApi api;
    dlerror();
    errno = 0;
    api.get = reinterpret_cast<GetTlsFunction>(dlsym(handle, "GetTLS"));
    const char *errorPointer = dlerror();
    if (api.get == nullptr || errorPointer != nullptr) {
        api.savedErrno = errno;
        api.error = errorPointer == nullptr ? "GetTLS resolved to null" : errorPointer;
        return api;
    }
    dlerror();
    errno = 0;
    api.set = reinterpret_cast<SetTlsFunction>(dlsym(handle, "SetTLS"));
    errorPointer = dlerror();
    if (api.set == nullptr || errorPointer != nullptr) {
        api.savedErrno = errno;
        api.error = errorPointer == nullptr ? "SetTLS resolved to null" : errorPointer;
        return api;
    }
    dlerror();
    errno = 0;
    api.reset = reinterpret_cast<ResetTlsFunction>(dlsym(handle, "ResetTLS"));
    errorPointer = dlerror();
    if (api.reset == nullptr || errorPointer != nullptr) {
        api.savedErrno = errno;
        api.error = errorPointer == nullptr ? "ResetTLS resolved to null" : errorPointer;
    }
    return api;
}

void SignalAbort(ThreadGate &gate) {
    pthread_mutex_lock(&gate.mutex);
    gate.abort = true;
    pthread_cond_broadcast(&gate.condition);
    pthread_mutex_unlock(&gate.mutex);
}

void *RunTlsWorker(void *opaque) {
    auto *context = static_cast<ThreadContext *>(opaque);
    ThreadGate &gate = *context->gate;
    pthread_mutex_lock(&gate.mutex);
    ++gate.waitingWorkers;
    pthread_cond_broadcast(&gate.condition);
    while (!gate.handleReady && !gate.abort) {
        pthread_cond_wait(&gate.condition, &gate.mutex);
    }
    if (gate.abort) {
        context->result.detail = std::string(context->role) + " aborted before symbol resolution";
        pthread_mutex_unlock(&gate.mutex);
        return nullptr;
    }
    void *handle = gate.handle;
    pthread_mutex_unlock(&gate.mutex);

    const TlsApi api = ResolveTlsApi(handle);
    context->result.resolved = api.get != nullptr && api.set != nullptr && api.reset != nullptr && api.error.empty();
    if (context->result.resolved) {
        context->result.initialValue = api.get();
    } else {
        context->result.detail = std::string(context->role) + " dlsym failed: " + api.error;
    }

    pthread_mutex_lock(&gate.mutex);
    ++gate.readyWorkers;
    pthread_cond_broadcast(&gate.condition);
    while (!gate.start && !gate.abort) {
        pthread_cond_wait(&gate.condition, &gate.mutex);
    }
    const bool aborted = gate.abort;
    pthread_mutex_unlock(&gate.mutex);
    if (aborted || !context->result.resolved) {
        return nullptr;
    }
    if (context->result.initialValue != kInitialValue) {
        context->result.detail = std::string(context->role) + " initial value expected 42, got " +
                                 std::to_string(context->result.initialValue);
        return nullptr;
    }

    for (int iteration = 0; iteration < kIterations; ++iteration) {
        api.set(context->value);
        if (api.get() != context->value) {
            context->result.detail = std::string(context->role) + " TLS isolation mismatch at iteration " +
                                     std::to_string(iteration + 1);
            return nullptr;
        }
        ++context->result.iterations;
    }
    api.reset();
    context->result.resetValue = api.get();
    if (context->result.resetValue != kInitialValue) {
        context->result.detail = std::string(context->role) + " reset expected 42, got " +
                                 std::to_string(context->result.resetValue);
        return nullptr;
    }
    context->result.ok = true;
    context->result.detail = std::string(context->role) + " initial=42, 100 set/read cycles, reset=42";
    return nullptr;
}

void PopulateThreadResults(ModelResult &result, const ThreadContext &preThread, const ThreadContext &postThread) {
    result.preThreadInitial = preThread.result.initialValue;
    result.postThreadInitial = postThread.result.initialValue;
    result.preThreadReset = preThread.result.resetValue;
    result.postThreadReset = postThread.result.resetValue;
    result.preThreadIterations = preThread.result.iterations;
    result.postThreadIterations = postThread.result.iterations;
}

void LogModelResult(const ModelResult &result) {
    OH_LOG_Print(LOG_APP, result.ok || result.expectedBlock ? LOG_INFO : LOG_ERROR, kLogDomain, kLogTag,
                 "TLS_MODEL_RESULT model=%{public}s library=%{public}s functional=%{public}s stage=%{public}s "
                 "mainInitial=%{public}d preInitial=%{public}d postInitial=%{public}d "
                 "mainIterations=%{public}d preIterations=%{public}d postIterations=%{public}d "
                 "mainReset=%{public}d preReset=%{public}d postReset=%{public}d detail=%{public}s "
                 "loaderError=%{public}s",
                 result.model.c_str(), result.library.c_str(), result.functionalStatus.c_str(), result.stage.c_str(),
                 result.mainInitial, result.preThreadInitial, result.postThreadInitial, result.mainIterations,
                 result.preThreadIterations, result.postThreadIterations, result.mainReset, result.preThreadReset,
                 result.postThreadReset, result.detail.c_str(), result.loaderError.c_str());
}

ModelResult ExecuteModel(const char *model, const char *library, bool required, bool expectInitialExecBlock,
                         int valueBase, size_t handleIndex) {
    ModelResult result;
    result.attempted = true;
    result.required = required;
    result.model = model;
    result.library = library;
    result.functionalStatus = "FAIL";
    result.stage = "pre-thread-create";
    result.detail = "starting TLS model probe";
    result.mainValue = valueBase + 1;
    result.preThreadValue = valueBase + 2;
    result.postThreadValue = valueBase + 3;

    ThreadGate gate{};
    if (pthread_mutex_init(&gate.mutex, nullptr) != 0 || pthread_cond_init(&gate.condition, nullptr) != 0) {
        result.detail = "failed to initialize pthread synchronization";
        LogModelResult(result);
        return result;
    }

    ThreadContext preThread{&gate, "pre-dlopen thread", result.preThreadValue, {}};
    ThreadContext postThread{&gate, "post-dlopen thread", result.postThreadValue, {}};
    pthread_t preThreadId{};
    pthread_t postThreadId{};
    if (pthread_create(&preThreadId, nullptr, RunTlsWorker, &preThread) != 0) {
        result.detail = "pthread_create failed for pre-dlopen waiting thread";
        pthread_cond_destroy(&gate.condition);
        pthread_mutex_destroy(&gate.mutex);
        LogModelResult(result);
        return result;
    }
    result.preThreadCreatedBeforeDlopen = true;
    pthread_mutex_lock(&gate.mutex);
    while (gate.waitingWorkers < 1) {
        pthread_cond_wait(&gate.condition, &gate.mutex);
    }
    pthread_mutex_unlock(&gate.mutex);

    result.stage = "dlopen";
    const LoaderObservation open = ObserveDlopen(library);
    result.loaderError = open.error;
    result.loaderErrno = open.savedErrno;
    if (open.address == nullptr || !open.error.empty()) {
        result.expectedBlock = expectInitialExecBlock &&
                               open.error.find(kInitialExecTlsRejection) != std::string::npos;
        result.functionalStatus = result.expectedBlock ? "BLOCKED" : "FAIL";
        result.detail = LoaderDetail("dlopen failed", open.error, open.savedErrno);
        SignalAbort(gate);
        pthread_join(preThreadId, nullptr);
        pthread_cond_destroy(&gate.condition);
        pthread_mutex_destroy(&gate.mutex);
        LogModelResult(result);
        return result;
    }
    g_retainedHandles[handleIndex] = open.address;

    if (expectInitialExecBlock) {
        result.environmentDrift = true;
        result.functionalStatus = "DRIFT";
        result.detail = "initial-exec library unexpectedly loaded; dynamic models were not attempted";
        SignalAbort(gate);
        pthread_join(preThreadId, nullptr);
        pthread_cond_destroy(&gate.condition);
        pthread_mutex_destroy(&gate.mutex);
        LogModelResult(result);
        return result;
    }

    pthread_mutex_lock(&gate.mutex);
    gate.handle = open.address;
    gate.handleReady = true;
    pthread_cond_broadcast(&gate.condition);
    pthread_mutex_unlock(&gate.mutex);

    result.stage = "post-thread-create";
    if (pthread_create(&postThreadId, nullptr, RunTlsWorker, &postThread) != 0) {
        result.detail = "pthread_create failed for post-dlopen thread";
        SignalAbort(gate);
        pthread_join(preThreadId, nullptr);
        pthread_cond_destroy(&gate.condition);
        pthread_mutex_destroy(&gate.mutex);
        LogModelResult(result);
        return result;
    }
    result.postThreadCreatedAfterDlopen = true;

    result.stage = "main-dlsym";
    const TlsApi mainApi = ResolveTlsApi(open.address);
    if (mainApi.get == nullptr || mainApi.set == nullptr || mainApi.reset == nullptr || !mainApi.error.empty()) {
        result.detail = "main thread dlsym failed: " + mainApi.error;
        SignalAbort(gate);
        pthread_join(preThreadId, nullptr);
        pthread_join(postThreadId, nullptr);
        PopulateThreadResults(result, preThread, postThread);
        pthread_cond_destroy(&gate.condition);
        pthread_mutex_destroy(&gate.mutex);
        LogModelResult(result);
        return result;
    }
    result.mainInitial = mainApi.get();

    pthread_mutex_lock(&gate.mutex);
    while (gate.readyWorkers < 2) {
        pthread_cond_wait(&gate.condition, &gate.mutex);
    }
    gate.start = true;
    pthread_cond_broadcast(&gate.condition);
    pthread_mutex_unlock(&gate.mutex);

    result.stage = "thread-isolation";
    if (result.mainInitial == kInitialValue) {
        for (int iteration = 0; iteration < kIterations; ++iteration) {
            mainApi.set(result.mainValue);
            if (mainApi.get() != result.mainValue) {
                result.detail = "main thread TLS isolation mismatch at iteration " + std::to_string(iteration + 1);
                break;
            }
            ++result.mainIterations;
        }
    } else {
        result.detail = "main thread initial value expected 42, got " + std::to_string(result.mainInitial);
    }

    pthread_join(preThreadId, nullptr);
    pthread_join(postThreadId, nullptr);
    mainApi.reset();
    result.mainReset = mainApi.get();
    PopulateThreadResults(result, preThread, postThread);

    const bool mainPassed = result.mainInitial == kInitialValue && result.mainIterations == kIterations &&
                            result.mainReset == kInitialValue;
    result.ok = mainPassed && preThread.result.ok && postThread.result.ok;
    result.functionalStatus = result.ok ? "PASS" : "FAIL";
    result.stage = result.ok ? "complete" : "thread-isolation";
    if (result.ok) {
        result.detail = "pre-dlopen, post-dlopen, and main threads each passed initial=42, 100 distinct set/read cycles, and reset=42";
    } else if (mainPassed && !preThread.result.ok) {
        result.detail = preThread.result.detail;
    } else if (mainPassed && !postThread.result.ok) {
        result.detail = postThread.result.detail;
    }

    pthread_cond_destroy(&gate.condition);
    pthread_mutex_destroy(&gate.mutex);
    LogModelResult(result);
    return result;
}

SuiteResult ExecuteTlsSuite() {
    SuiteResult result;
    result.initialExec = ExecuteModel("initial-exec", "libtls-ie.so", true, true, 1000, 0);
    result.ieBlocked = result.initialExec.expectedBlock;
    result.environmentDrift = result.initialExec.environmentDrift;
    if (!result.ieBlocked) {
        result.verdict = result.environmentDrift ? "DRIFT" : "FAIL";
        result.stopReason = result.environmentDrift ? "initial-exec unexpectedly loaded; environment drift" :
                                                     "initial-exec did not reproduce the expected loader rejection";
        return result;
    }

    result.globalDynamic = ExecuteModel("classic-global-dynamic", "libtls-gd.so", true, false, 2000, 1);
    result.tlsDesc = ExecuteModel("tlsdesc-gnu2", "libtls-desc.so", true, false, 3000, 2);
    result.localDynamic = ExecuteModel("local-dynamic", "libtls-ld.so", false, false, 4000, 3);
    result.tier1Pass = result.globalDynamic.ok || result.tlsDesc.ok;
    result.ok = result.tier1Pass;
    result.verdict = result.tier1Pass ? "PASS" : "STOP";
    result.stopReason = result.tier1Pass ? "classic GD or TLSDESC passed the Tier1 gate" :
                                          "classic GD and TLSDESC both failed; stop only API24 x86_64 tuple";
    OH_LOG_Print(LOG_APP, result.ok ? LOG_INFO : LOG_ERROR, kLogDomain, kLogTag,
                 "TLS_SUITE_RESULT verdict=%{public}s ieBlocked=%{public}d gd=%{public}s tlsdesc=%{public}s "
                 "localDynamic=%{public}s reason=%{public}s",
                 result.verdict.c_str(), result.ieBlocked, result.globalDynamic.functionalStatus.c_str(),
                 result.tlsDesc.functionalStatus.c_str(), result.localDynamic.functionalStatus.c_str(),
                 result.stopReason.c_str());
    return result;
}

napi_value MakeString(napi_env env, const char *value) {
    napi_value result = nullptr;
    if (napi_create_string_utf8(env, value, NAPI_AUTO_LENGTH, &result) != napi_ok) {
        return nullptr;
    }
    return result;
}

bool SetNamedValue(napi_env env, napi_value object, const char *name, napi_value value) {
    return value != nullptr && napi_set_named_property(env, object, name, value) == napi_ok;
}

bool SetBoolean(napi_env env, napi_value object, const char *name, bool value) {
    napi_value napiValue = nullptr;
    return napi_get_boolean(env, value, &napiValue) == napi_ok && SetNamedValue(env, object, name, napiValue);
}

bool SetInteger(napi_env env, napi_value object, const char *name, int value) {
    napi_value napiValue = nullptr;
    return napi_create_int32(env, value, &napiValue) == napi_ok && SetNamedValue(env, object, name, napiValue);
}

bool SetString(napi_env env, napi_value object, const char *name, const std::string &value) {
    return SetNamedValue(env, object, name, MakeString(env, value.c_str()));
}

napi_value MakeModelResult(napi_env env, const ModelResult &result) {
    napi_value object = nullptr;
    if (napi_create_object(env, &object) != napi_ok) {
        return nullptr;
    }
    const bool success = SetBoolean(env, object, "attempted", result.attempted) &&
                         SetBoolean(env, object, "required", result.required) &&
                         SetBoolean(env, object, "ok", result.ok) &&
                         SetBoolean(env, object, "expectedBlock", result.expectedBlock) &&
                         SetBoolean(env, object, "environmentDrift", result.environmentDrift) &&
                         SetBoolean(env, object, "preThreadCreatedBeforeDlopen", result.preThreadCreatedBeforeDlopen) &&
                         SetBoolean(env, object, "postThreadCreatedAfterDlopen", result.postThreadCreatedAfterDlopen) &&
                         SetString(env, object, "model", result.model) && SetString(env, object, "library", result.library) &&
                         SetString(env, object, "functionalStatus", result.functionalStatus) &&
                         SetString(env, object, "stage", result.stage) && SetString(env, object, "detail", result.detail) &&
                         SetString(env, object, "loaderError", result.loaderError) &&
                         SetInteger(env, object, "loaderErrno", result.loaderErrno) &&
                         SetInteger(env, object, "mainValue", result.mainValue) &&
                         SetInteger(env, object, "preThreadValue", result.preThreadValue) &&
                         SetInteger(env, object, "postThreadValue", result.postThreadValue) &&
                         SetInteger(env, object, "mainInitial", result.mainInitial) &&
                         SetInteger(env, object, "preThreadInitial", result.preThreadInitial) &&
                         SetInteger(env, object, "postThreadInitial", result.postThreadInitial) &&
                         SetInteger(env, object, "mainReset", result.mainReset) &&
                         SetInteger(env, object, "preThreadReset", result.preThreadReset) &&
                         SetInteger(env, object, "postThreadReset", result.postThreadReset) &&
                         SetInteger(env, object, "mainIterations", result.mainIterations) &&
                         SetInteger(env, object, "preThreadIterations", result.preThreadIterations) &&
                         SetInteger(env, object, "postThreadIterations", result.postThreadIterations);
    return success ? object : nullptr;
}

napi_value MakeSuiteResult(napi_env env, const SuiteResult &result) {
    napi_value object = nullptr;
    if (napi_create_object(env, &object) != napi_ok) {
        return nullptr;
    }
    const bool success = SetBoolean(env, object, "ok", result.ok) &&
                         SetBoolean(env, object, "ieBlocked", result.ieBlocked) &&
                         SetBoolean(env, object, "environmentDrift", result.environmentDrift) &&
                         SetBoolean(env, object, "tier1Pass", result.tier1Pass) &&
                         SetString(env, object, "verdict", result.verdict) &&
                         SetString(env, object, "stopReason", result.stopReason) &&
                         SetNamedValue(env, object, "initialExec", MakeModelResult(env, result.initialExec)) &&
                         SetNamedValue(env, object, "globalDynamic", MakeModelResult(env, result.globalDynamic)) &&
                         SetNamedValue(env, object, "tlsDesc", MakeModelResult(env, result.tlsDesc)) &&
                         SetNamedValue(env, object, "localDynamic", MakeModelResult(env, result.localDynamic));
    return success ? object : nullptr;
}

napi_value Ping(napi_env env, napi_callback_info info) {
    (void)info;
    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag, "Node-API ping invoked");
    return MakeString(env, "pong");
}

napi_value Version(napi_env env, napi_callback_info info) {
    (void)info;
    return MakeString(env, kProbeVersion);
}

napi_value RunDynamicTlsProbe(napi_env env, napi_callback_info info) {
    (void)info;
    if (g_suiteInvoked) {
        SuiteResult repeated;
        repeated.verdict = "FAIL";
        repeated.stopReason = "TLS suite may run only once per TestRunner process";
        return MakeSuiteResult(env, repeated);
    }
    g_suiteInvoked = true;
    return MakeSuiteResult(env, ExecuteTlsSuite());
}

napi_value Init(napi_env env, napi_value exports) {
    napi_property_descriptor properties[] = {
        {"ping", nullptr, Ping, nullptr, nullptr, nullptr, napi_default, nullptr},
        {"version", nullptr, Version, nullptr, nullptr, nullptr, napi_default, nullptr},
        {"runDynamicTlsProbe", nullptr, RunDynamicTlsProbe, nullptr, nullptr, nullptr, napi_default, nullptr},
    };
    if (napi_define_properties(env, exports, sizeof(properties) / sizeof(properties[0]), properties) != napi_ok) {
        return nullptr;
    }
    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag, "Node-API module initialized");
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
