#include <hilog/log.h>
#include <napi/native_api.h>

namespace {
constexpr unsigned int kLogDomain = 0x2900;
constexpr const char *kLogTag = "R1Api24Probe";
constexpr const char *kProbeVersion = "r1-api24-probe/0.0.1";

napi_value MakeString(napi_env env, const char *value) {
    napi_value result = nullptr;
    if (napi_create_string_utf8(env, value, NAPI_AUTO_LENGTH, &result) != napi_ok) {
        return nullptr;
    }
    return result;
}

napi_value Ping(napi_env env, napi_callback_info info) {
    (void)info;
    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag, "Node-API ping invoked");
    return MakeString(env, "pong");
}

napi_value Version(napi_env env, napi_callback_info info) {
    (void)info;
    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag, "Node-API version invoked");
    return MakeString(env, kProbeVersion);
}

napi_value Init(napi_env env, napi_value exports) {
    napi_property_descriptor properties[] = {
        {"ping", nullptr, Ping, nullptr, nullptr, nullptr, napi_default, nullptr},
        {"version", nullptr, Version, nullptr, nullptr, nullptr, napi_default, nullptr},
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
