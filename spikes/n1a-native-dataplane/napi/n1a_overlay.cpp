// N1a gate data-plane probe — thinnest C++ NAPI overlay (mirrors N0's
// napi/n0_overlay.cpp pattern).
//
// Links the Rust core (libn1acore.a, built by build.sh) and exports one
// synchronous ArkTS entry point: runN1aProbe().
//
// runN1aProbe() returns a structured object:
//   {
//     version: string,
//     ok: boolean,            // fail-closed verdict (true = pass)
//     verdict: string,        // "pass" | "fail"
//     c1..c9: string,         // "pass" | "fail" | "not-triggered" (C5 only)
//     verifiedPacketsTotal: number,
//     mismatchCount: number,
//     lostCount: number,
//     backpressureTriggered: boolean,
//     throughputMiBps: number,
//     pumpMs: number,
//     fdBaseline: number,
//     fdAfter: number,
//     detailJson: string      // full machine-readable JSON from the Rust core
//   }
//
// Emits exactly one single-line HiLog marker per call, with the C9-pinned
// field set (frozen criteria r2 — no other field may enter the marker):
//   N1A_RESULT|verdict=<PASS|FAIL>|c5=<induced|not-triggered|fail>|throughput_mibps=<x.xx>
// (tag "N1aProbe", domain 0x2900 — same domain as N0). All other result
// fields stay in the structured NAPI object and the JSON detail document.
//
// Fail-closed: any marshaling error throws a napi error; a failed Rust probe
// is reported with ok=false (never as success), and the raw JSON detail is
// always passed through for the evidence record.
//
// Scope (docs/n1a-gate-plan.md): no management/ICE/relay/UI/VPN/TUN/protect
// surface; aarch64 is cross-compile only, no load claim.

#include <napi/native_api.h>
#include <hilog/log.h>

#include <cstdint>
#include <cstring>
#include <string.h>

namespace {

constexpr unsigned int kLogDomain = 0x2900;
constexpr const char *kLogTag = "N1aProbe";

// Upper bound for reading the core's JSON string (the document is ~2 KB; a
// larger value means the terminator is missing — fail closed).
constexpr size_t kJsonMaxLen = 64 * 1024;

// Narrow C ABI of the Rust core (spikes/n1a-native-dataplane/src/lib.rs).
extern "C" {
    const char *n1a_probe_version(void);
    struct N1aDataplaneResult {
        int32_t ok;                    // 0 = all criteria pass (fail-closed)
        int32_t criteria[9];           // 1 = pass, 0 = fail, 2 = not-triggered
        int32_t verified_packets_total;
        int32_t mismatch_count;
        int32_t lost_count;
        int32_t backpressure_triggered;
        double throughput_mib_per_sec;
        double pump_ms;
        int32_t fd_baseline;
        int32_t fd_after;
        char *json;                    // NUL-terminated, owned by the result
    };
    N1aDataplaneResult *n1a_dataplane_probe(void);
    void n1a_result_free(N1aDataplaneResult *result);
}

// RAII guard so the Rust result is always freed exactly once, even on the
// many early-return error paths below.
struct ResultGuard {
    N1aDataplaneResult *ptr;
    ~ResultGuard() {
        if (ptr != nullptr) {
            n1a_result_free(ptr);
        }
    }
};

const char *CriterionName(int32_t status) {
    switch (status) {
        case 1:
            return "pass";
        case 2:
            return "not-triggered";
        default:
            return "fail";
    }
}

// C9 marker enumeration for the c5 field (frozen r2): the marker uses
// `induced` where the aggregation vocabulary says `pass-induced` and the
// structured object/JSON keep their own layer (`pass` / `pass-induced`).
const char *C5MarkerName(int32_t status) {
    switch (status) {
        case 1:
            return "induced";
        case 2:
            return "not-triggered";
        default:
            return "fail";
    }
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

bool SetNamedDouble(napi_env env, napi_value object, const char *name, double value) {
    napi_value property = nullptr;
    return napi_create_double(env, value, &property) == napi_ok &&
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

bool SetNamedStringLen(napi_env env, napi_value object, const char *name,
                       const char *value, size_t len) {
    napi_value property = nullptr;
    return napi_create_string_utf8(env, value, len, &property) == napi_ok &&
        napi_set_named_property(env, object, name, property) == napi_ok;
}

napi_value RunN1aProbe(napi_env env, napi_callback_info info) {
    (void)info;

    const char *version = n1a_probe_version();
    if (version == nullptr) {
        return ThrowError(env, "n1a_probe_version returned null");
    }

    ResultGuard guard;
    guard.ptr = n1a_dataplane_probe();
    if (guard.ptr == nullptr) {
        return ThrowError(env, "n1a_dataplane_probe returned null");
    }
    N1aDataplaneResult *probe = guard.ptr;

    // The JSON document must be a NUL-terminated string within a sane bound.
    const size_t json_len = strnlen(probe->json, kJsonMaxLen);
    if (json_len == 0 || json_len == kJsonMaxLen) {
        return ThrowError(env, "probe json is empty or not NUL-terminated");
    }

    // Fail-closed consistency: criterion statuses must be in range, and
    // ok == 0 must hold exactly when no criterion failed.
    bool any_failed = false;
    for (int i = 0; i < 9; i++) {
        const int32_t status = probe->criteria[i];
        if (status != 0 && status != 1 && status != 2) {
            return ThrowError(env, "probe criterion status out of range");
        }
        if (status == 0) {
            any_failed = true;
        }
    }
    // C5 (index 4) may be 2 = not-triggered without failing the verdict.
    bool verdict_consistent = (probe->ok == 0) == !any_failed;
    if (!verdict_consistent) {
        return ThrowError(env, "probe verdict inconsistent with criterion statuses");
    }
    if (probe->ok == 0 && (probe->verified_packets_total != 2000 ||
                           probe->mismatch_count != 0 || probe->lost_count != 0)) {
        return ThrowError(env, "probe verdict inconsistent with integrity counters");
    }

    napi_value result = nullptr;
    if (napi_create_object(env, &result) != napi_ok) {
        return ThrowError(env, "failed to create runN1aProbe result");
    }
    if (!SetNamedString(env, result, "version", version) ||
        !SetNamedBool(env, result, "ok", probe->ok == 0) ||
        !SetNamedString(env, result, "verdict", probe->ok == 0 ? "pass" : "fail")) {
        return ThrowError(env, "failed to build runN1aProbe result");
    }
    for (int i = 0; i < 9; i++) {
        char name[4] = {'c', static_cast<char>('1' + i), 0};
        // The structured object uses the C9/marker enumeration for c5
        // (induced / not-triggered / fail); the JSON detail keeps the
        // aggregation vocabulary (pass-induced) — both layers are the
        // same underlying status, never a fourth state.
        const char *value = (i == 4) ? C5MarkerName(probe->criteria[i])
                                    : CriterionName(probe->criteria[i]);
        if (!SetNamedString(env, result, name, value)) {
            return ThrowError(env, "failed to build runN1aProbe result");
        }
    }
    if (!SetNamedInt32(env, result, "verifiedPacketsTotal", probe->verified_packets_total) ||
        !SetNamedInt32(env, result, "mismatchCount", probe->mismatch_count) ||
        !SetNamedInt32(env, result, "lostCount", probe->lost_count) ||
        !SetNamedBool(env, result, "backpressureTriggered",
                      probe->backpressure_triggered == 1) ||
        !SetNamedDouble(env, result, "throughputMiBps", probe->throughput_mib_per_sec) ||
        !SetNamedDouble(env, result, "pumpMs", probe->pump_ms) ||
        !SetNamedInt32(env, result, "fdBaseline", probe->fd_baseline) ||
        !SetNamedInt32(env, result, "fdAfter", probe->fd_after) ||
        !SetNamedStringLen(env, result, "detailJson", probe->json, json_len)) {
        return ThrowError(env, "failed to build runN1aProbe result");
    }

    // Single-line machine-readable marker with the C9-pinned field set
    // (frozen criteria r2): exactly four fields, no more. verdict is
    // PASS/FAIL (uppercase); c5 uses the C9 enumeration induced /
    // not-triggered / fail; throughput_mibps is printed with two decimals.
    // All %{public} — no key material is ever logged.
    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
        "N1A_RESULT|verdict=%{public}s|c5=%{public}s|throughput_mibps=%{public}.2f",
        probe->ok == 0 ? "PASS" : "FAIL",
        C5MarkerName(probe->criteria[4]),
        probe->throughput_mib_per_sec);
    return result;
}

napi_value Init(napi_env env, napi_value exports) {
    napi_property_descriptor properties[] = {
        {"runN1aProbe", nullptr, RunN1aProbe, nullptr, nullptr, nullptr, napi_default, nullptr},
    };
    if (napi_define_properties(env, exports, sizeof(properties) / sizeof(properties[0]),
            properties) != napi_ok) {
        return nullptr;
    }
    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag, "N1a overlay module initialized");
    return exports;
}

napi_module g_n1aModule = {
    1,
    0,
    nullptr,
    Init,
    "entry",
    nullptr,
    {0},
};
} // namespace

extern "C" __attribute__((constructor, visibility("default"))) void RegisterN1aModule() {
    napi_module_register(&g_n1aModule);
}
