// N0 native core — thinnest C++ NAPI overlay.
//
// Links the Rust core (libn0core.a, built by build.sh) and exports one ArkTS
// entry point: runProbe().
//
// runProbe() returns structured fields:
//   { version: string, key: string, smoke: { ok, x25519Ok, tunnelOk, tickOp, tickSize } }
//
// Fail-closed: any marshaling error throws a napi error; a failed Rust smoke is
// reported with smoke.ok = false (never reported as success).
//
// Scope (docs/n0-native-client-feasibility.md, N0(b)): no management/ICE/relay/
// UI/VPN/TUN/protect surface; arm64 is cross-compile only, no load claim.

#include <napi/native_api.h>
#include <hilog/log.h>

#include <cstdint>
#include <cstring>
#include <string.h>

namespace {

constexpr unsigned int kLogDomain = 0x2900;
constexpr const char *kLogTag = "N0NativeCore";

// Narrow C ABI of the Rust core (spikes/n0-native-core/src/lib.rs).
extern "C" {
    const char *n0_probe_version(void);
    struct N0SmokeResult {
        int32_t ok;          // 0 = all smoke checks passed
        int32_t x25519_ok;   // 1 = x25519 keygen/derive/base64/check passed
        int32_t tunnel_ok;   // 1 = new_tunnel + wireguard_tick passed
        int32_t tick_op;     // last wireguard_tick op (boringtun result_type)
        uint64_t tick_size;  // last wireguard_tick size
        char public_key_b64[64];  // base64 of derived public key, NUL-terminated
    };
    int32_t n0_probe_smoke(N0SmokeResult *out);
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

bool SetNamedUint64(napi_env env, napi_value object, const char *name, uint64_t value) {
    napi_value property = nullptr;
    return napi_create_double(env, static_cast<double>(value), &property) == napi_ok &&
        napi_set_named_property(env, object, name, property) == napi_ok;
}

bool SetNamedBool(napi_env env, napi_value object, const char *name, bool value) {
    napi_value property = nullptr;
    return napi_get_boolean(env, value, &property) == napi_ok &&
        napi_set_named_property(env, object, name, property) == napi_ok;
}

bool SetNamedString(napi_env env, napi_value object, const char *name, const char *value) {
    napi_value property = nullptr;
    return napi_create_string_utf8(env, value, NAPI_AUTO_LENGTH, &property) == napi_ok &&
        napi_set_named_property(env, object, name, property) == napi_ok;
}

bool SetNamedStringLen(napi_env env, napi_value object, const char *name, const char *value, size_t len) {
    napi_value property = nullptr;
    return napi_create_string_utf8(env, value, len, &property) == napi_ok &&
        napi_set_named_property(env, object, name, property) == napi_ok;
}

napi_value RunProbe(napi_env env, napi_callback_info info) {
    (void)info;

    const char *version = n0_probe_version();
    if (version == nullptr) {
        return ThrowError(env, "n0_probe_version returned null");
    }

    N0SmokeResult smoke = {};
    const int32_t rc = n0_probe_smoke(&smoke);
    // n0_probe_smoke returns out->ok by contract, so rc == smoke.ok must hold.
    // In the C ABI, ok == 0 means PASS (a host check reading `rc=0 ok=0` must
    // read it as pass, not as a contradiction); a nonzero ok means the smoke
    // failed and is reported as smoke.ok=false in ArkTS. Any disagreement
    // between the return code and the struct field is an inconsistency: fail
    // closed.
    if (rc != smoke.ok) {
        return ThrowError(env, "n0_probe_smoke returned inconsistent status");
    }

    napi_value result = nullptr;
    napi_value smokeObject = nullptr;
    // The public key buffer is a fixed 64-byte array; read its length with a
    // strnlen upper bound so a missing NUL terminator can never make the
    // string read past the buffer (fail-closed: reject instead).
    const size_t key_len = strnlen(smoke.public_key_b64, sizeof(smoke.public_key_b64));
    if (key_len == sizeof(smoke.public_key_b64)) {
        return ThrowError(env, "public key buffer not NUL-terminated");
    }
    if (napi_create_object(env, &result) != napi_ok ||
        napi_create_object(env, &smokeObject) != napi_ok ||
        !SetNamedString(env, result, "version", version) ||
        !SetNamedStringLen(env, result, "key", smoke.public_key_b64, key_len) ||
        !SetNamedBool(env, smokeObject, "ok", smoke.ok == 0) ||
        !SetNamedBool(env, smokeObject, "x25519Ok", smoke.x25519_ok == 1) ||
        !SetNamedBool(env, smokeObject, "tunnelOk", smoke.tunnel_ok == 1) ||
        !SetNamedInt32(env, smokeObject, "tickOp", smoke.tick_op) ||
        !SetNamedUint64(env, smokeObject, "tickSize", smoke.tick_size) ||
        napi_set_named_property(env, result, "smoke", smokeObject) != napi_ok) {
        return ThrowError(env, "failed to build runProbe result");
    }

    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
        "N0_RUNPROBE|version=%{public}s|smokeOk=%{public}d|x25519Ok=%{public}d|"
        "tunnelOk=%{public}d|tickOp=%{public}d|tickSize=%{public}llu",
        version, smoke.ok == 0 ? 1 : 0, smoke.x25519_ok, smoke.tunnel_ok, smoke.tick_op,
        static_cast<unsigned long long>(smoke.tick_size));
    return result;
}

napi_value Init(napi_env env, napi_value exports) {
    napi_property_descriptor properties[] = {
        {"runProbe", nullptr, RunProbe, nullptr, nullptr, nullptr, napi_default, nullptr},
    };
    if (napi_define_properties(env, exports, sizeof(properties) / sizeof(properties[0]),
            properties) != napi_ok) {
        return nullptr;
    }
    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag, "N0 overlay module initialized");
    return exports;
}

napi_module g_n0Module = {
    1,
    0,
    nullptr,
    Init,
    "entry",
    nullptr,
    {0},
};
} // namespace

extern "C" __attribute__((constructor, visibility("default"))) void RegisterN0Module() {
    napi_module_register(&g_n0Module);
}
