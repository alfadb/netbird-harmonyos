// N1a gate data-plane probe — thinnest C++ NAPI overlay (mirrors N0's
// napi/n0_overlay.cpp pattern).
//
// Links the Rust core (libn1acore.a, built by build.sh) and exports one
// synchronous ArkTS entry point: runN1aProbe().
//
// runN1aProbe(processModel?: string) returns a structured object:
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
//     fd2Closed: boolean,     // r3 C7(1): both probe socket fds closed at T3
//     fdSetDiffCount: number, // r3 C7: |T3 fd set - T0 fd set| (observation)
//     newTidsObserved: number,// r3 C7(2): new TIDs in window (observation)
//     tunnelsFreed: number,   // r3 C8: tunnel_free count (must be 2)
//     processModel: string,   // r3 C7(3): "testrunner"|"entryability"|"unknown"
//     detailJson: string,     // full machine-readable JSON from the Rust core
//     detailSha256: string    // 64-hex SHA-256 of detailJson (transport
//                             // integrity; defect 2 of EV-N1A-...-0001)
//   }
//
// Emits TWO single-line HiLog markers per call (tag "N1aProbe", domain
// 0x2900 — same domain as N0):
//   1. The C9-pinned four-field set (frozen r2/r3 — no other field may enter):
//      N1A_RESULT|verdict=<PASS|FAIL>|c5=<induced|not-triggered|fail>|throughput_mibps=<x.xx>
//   2. The r3 C7/C8 evidence short marker (distinct prefix — never uses the
//      N1A_RESULT prefix, per the r3 re-review; single line, no pipes in values):
//      N1A_RES|c7=<pass|fail>|c8=<pass|fail>|fd2=<closed|open>|fdset=<diff-count>
// All other result fields stay in the structured NAPI object and the JSON
// detail document.
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
#include <cstdio>
#include <cstring>
#include <string.h>

namespace {

constexpr unsigned int kLogDomain = 0x2900;
constexpr const char *kLogTag = "N1aProbe";

// Upper bound for reading the core's JSON string (the document is ~2 KB; a
// larger value means the terminator is missing — fail closed).
constexpr size_t kJsonMaxLen = 64 * 1024;
// The Rust C ABI carries the digest as [u8; 65] (64 lowercase hex + NUL).
constexpr size_t kDetailShaHexLen = 64;

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
        int32_t c7_fd2_closed;         // r3: 1 = both probe sockets closed at T3
        int32_t c7_fdset_diff_count;   // r3: |T3 fd set - T0 set| (observation)
        int32_t c7_new_tids;           // r3: new TIDs in window (observation)
        int32_t c8_tunnels_freed;      // r3: must be 2
        unsigned char process_model[32]; // r3: NUL-terminated label
        char *json;                    // NUL-terminated, owned by the result
        unsigned char detail_sha256[65]; // 64 lowercase hex chars + NUL
    };
    N1aDataplaneResult *n1a_dataplane_probe(const char *process_model);
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

// Diagnostic snapshot (observability defect #3 of EV-N1A-EMU24-20260831-0001):
// every ThrowError site MUST call EmitDiagnosticSnapshot first so the probe's
// real field values survive on the abnormal path. The runner collects these
// lines into <id>-overlay-diag.log (evidence channel; not a gate).
constexpr size_t kDiagJsonChunk = 380; // ≤380-byte JSON chunks (≤488 hilog cap)

void EmitDiagnosticSnapshot(const N1aDataplaneResult *probe, const char *stage) {
    if (probe == nullptr) {
        OH_LOG_Print(LOG_APP, LOG_ERROR, kLogDomain, kLogTag,
            "N1A_DIAG|stage=%{public}s|probe=null", stage);
        return;
    }
    OH_LOG_Print(LOG_APP, LOG_ERROR, kLogDomain, kLogTag,
        "N1A_DIAG|stage=%{public}s|ok=%{public}d|"
        "c=[%{public}d,%{public}d,%{public}d,%{public}d,%{public}d,"
        "%{public}d,%{public}d,%{public}d,%{public}d]|"
        "v=%{public}d|mm=%{public}d|lost=%{public}d|bp=%{public}d|"
        "c7fd2=%{public}d|c7set=%{public}d|c7tid=%{public}d|c8free=%{public}d|"
        "pm=%{public}s",
        stage,
        probe->ok,
        probe->criteria[0], probe->criteria[1], probe->criteria[2],
        probe->criteria[3], probe->criteria[4], probe->criteria[5],
        probe->criteria[6], probe->criteria[7], probe->criteria[8],
        probe->verified_packets_total, probe->mismatch_count,
        probe->lost_count, probe->backpressure_triggered,
        probe->c7_fd2_closed, probe->c7_fdset_diff_count,
        probe->c7_new_tids, probe->c8_tunnels_freed,
        reinterpret_cast<const char *>(probe->process_model));
    // Chunked JSON block: the full detail document, ≤380 bytes per line.
    // The JSON is verified single-line ASCII by a Rust unit test
    // (probe_json_is_transport_safe_ascii_single_line_no_pipe), so byte
    // slicing is safe. The digest's first 16 hex chars identify the block.
    const size_t json_len = strnlen(probe->json, kJsonMaxLen);
    if (json_len > 0 && json_len < kJsonMaxLen) {
        char sha16[17];
        for (int i = 0; i < 16 && probe->detail_sha256[i] != 0; i++) {
            sha16[i] = static_cast<char>(probe->detail_sha256[i]);
        }
        sha16[16] = '\0';
        OH_LOG_Print(LOG_APP, LOG_ERROR, kLogDomain, kLogTag,
            "N1A_DIAG_JSON_BEG|sha=%{public}s", sha16);
        for (size_t off = 0; off < json_len; off += kDiagJsonChunk) {
            const size_t n = (json_len - off < kDiagJsonChunk)
                ? (json_len - off) : kDiagJsonChunk;
            OH_LOG_Print(LOG_APP, LOG_ERROR, kLogDomain, kLogTag,
                "N1A_DIAG_JSON|%{public}.*s", static_cast<int>(n),
                probe->json + off);
        }
        OH_LOG_Print(LOG_APP, LOG_ERROR, kLogDomain, kLogTag,
            "N1A_DIAG_JSON_END|sha=%{public}s|bytes=%{public}zu", sha16, json_len);
    }
}

// Formats the key fields as a bracketed suffix for the ThrowError message
// (defect #3: even if hilog is lost, the exception message carries the fields).
// Returns a static buffer — safe because ThrowError never returns.
const char *FieldSuffix(const N1aDataplaneResult *probe) {
    static char buf[192];
    if (probe == nullptr) {
        snprintf(buf, sizeof(buf), " [probe=null]");
        return buf;
    }
    snprintf(buf, sizeof(buf),
        " [ok=%d c=[%d,%d,%d,%d,%d,%d,%d,%d,%d] v=%d mm=%d lost=%d "
        "c7fd2=%d c8free=%d pm=%s]",
        probe->ok,
        probe->criteria[0], probe->criteria[1], probe->criteria[2],
        probe->criteria[3], probe->criteria[4], probe->criteria[5],
        probe->criteria[6], probe->criteria[7], probe->criteria[8],
        probe->verified_packets_total, probe->mismatch_count,
        probe->lost_count, probe->c7_fd2_closed, probe->c8_tunnels_freed,
        reinterpret_cast<const char *>(probe->process_model));
    return buf;
}

napi_value ThrowError(napi_env env, const char *message) {
    napi_throw_error(env, nullptr, message);
    return nullptr;
}

// Convenience wrapper: emit the diagnostic snapshot, then throw with the
// field suffix appended. Every abnormal path in RunN1aProbe uses this.
napi_value DiagAndThrow(napi_env env, const N1aDataplaneResult *probe,
                        const char *stage, const char *message) {
    EmitDiagnosticSnapshot(probe, stage);
    char msg[512];
    snprintf(msg, sizeof(msg), "%s%s", message, FieldSuffix(probe));
    return ThrowError(env, msg);
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
    // r3 C7(3): optional process-model label (observation-only). The ohosTest
    // entry passes "testrunner"; the phase-B result page passes
    // "entryability"; absence or a non-string argument -> NULL -> "unknown".
    const char *process_model_arg = nullptr;
    char process_model_buf[32];
    size_t argc = 0;
    napi_value argv[1] = {nullptr};
    if (napi_get_cb_info(env, info, &argc, argv, nullptr, nullptr) == napi_ok && argc >= 1) {
        napi_valuetype type = napi_undefined;
        if (napi_typeof(env, argv[0], &type) == napi_ok && type == napi_string) {
            size_t copied = 0;
            if (napi_get_value_string_utf8(env, argv[0], process_model_buf,
                    sizeof(process_model_buf), &copied) == napi_ok) {
                process_model_arg = process_model_buf;
            }
        }
    }

    const char *version = n1a_probe_version();
    if (version == nullptr) {
        return DiagAndThrow(env, nullptr, "version-null", "n1a_probe_version returned null");
    }

    ResultGuard guard;
    guard.ptr = n1a_dataplane_probe(process_model_arg);
    if (guard.ptr == nullptr) {
        return DiagAndThrow(env, nullptr, "probe-null", "n1a_dataplane_probe returned null");
    }
    N1aDataplaneResult *probe = guard.ptr;

    // The JSON document must be a NUL-terminated string within a sane bound.
    const size_t json_len = strnlen(probe->json, kJsonMaxLen);
    if (json_len == 0 || json_len == kJsonMaxLen) {
        return DiagAndThrow(env, probe, "json-bad",
            "probe json is empty or not NUL-terminated");
    }
    // Defect 2 transport digest: NUL-terminated lowercase hex (64 chars).
    char detail_sha[kDetailShaHexLen + 1];
    size_t sha_len = 0;
    while (sha_len < kDetailShaHexLen && probe->detail_sha256[sha_len] != 0) {
        detail_sha[sha_len] = static_cast<char>(probe->detail_sha256[sha_len]);
        sha_len++;
    }
    detail_sha[sha_len] = '\0';
    if (sha_len != kDetailShaHexLen) {
        return DiagAndThrow(env, probe, "sha-bad",
            "probe detail_sha256 is not 64 hex chars");
    }

    // Fail-closed consistency: criterion statuses must be in range, and
    // ok == 0 must hold exactly when no criterion failed.
    bool any_failed = false;
    for (int i = 0; i < 9; i++) {
        const int32_t status = probe->criteria[i];
        if (status != 0 && status != 1 && status != 2) {
            return DiagAndThrow(env, probe, "criterion-range",
                "probe criterion status out of range");
        }
        if (status == 0) {
            any_failed = true;
        }
    }
    // C5 (index 4) may be 2 = not-triggered without failing the verdict.
    bool verdict_consistent = (probe->ok == 0) == !any_failed;
    if (!verdict_consistent) {
        return DiagAndThrow(env, probe, "verdict-criteria-mismatch",
            "probe verdict inconsistent with criterion statuses");
    }
    if (probe->ok == 0 && (probe->verified_packets_total != 2000 ||
                           probe->mismatch_count != 0 || probe->lost_count != 0)) {
        return DiagAndThrow(env, probe, "verdict-integrity-mismatch",
            "probe verdict inconsistent with integrity counters");
    }

    napi_value result = nullptr;
    if (napi_create_object(env, &result) != napi_ok) {
        return DiagAndThrow(env, probe, "create-object",
            "failed to create runN1aProbe result");
    }
    if (!SetNamedString(env, result, "version", version) ||
        !SetNamedBool(env, result, "ok", probe->ok == 0) ||
        !SetNamedString(env, result, "verdict", probe->ok == 0 ? "pass" : "fail")) {
        return DiagAndThrow(env, probe, "build-basic",
            "failed to build runN1aProbe result");
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
            return DiagAndThrow(env, probe, "build-criteria",
                "failed to build runN1aProbe result");
        }
    }
    if (!SetNamedInt32(env, result, "verifiedPacketsTotal", probe->verified_packets_total) ||
        !SetNamedInt32(env, result, "mismatchCount", probe->mismatch_count) ||
        !SetNamedInt32(env, result, "lostCount", probe->lost_count) ||
        !SetNamedBool(env, result, "backpressureTriggered",
                      probe->backpressure_triggered == 1) ||
        !SetNamedDouble(env, result, "throughputMiBps", probe->throughput_mib_per_sec) ||
        !SetNamedDouble(env, result, "pumpMs", probe->pump_ms) ||
        // r3 C7/C8 evidence scalars (implementation-layer requirement: fd
        // verification results reach evidence via short marker/NAPI scalars,
        // never only via the hilog-truncated single-line detailJson).
        !SetNamedBool(env, result, "fd2Closed", probe->c7_fd2_closed == 1) ||
        !SetNamedInt32(env, result, "fdSetDiffCount", probe->c7_fdset_diff_count) ||
        !SetNamedInt32(env, result, "newTidsObserved", probe->c7_new_tids) ||
        !SetNamedInt32(env, result, "tunnelsFreed", probe->c8_tunnels_freed) ||
        !SetNamedString(env, result, "processModel",
                        reinterpret_cast<const char *>(probe->process_model)) ||
        !SetNamedStringLen(env, result, "detailJson", probe->json, json_len) ||
        !SetNamedStringLen(env, result, "detailSha256", detail_sha, sha_len)) {
        return DiagAndThrow(env, probe, "build-scalars",
            "failed to build runN1aProbe result");
    }

    // Single-line machine-readable marker with the C9-pinned field set
    // (frozen criteria r2/r3): exactly four fields, no more. verdict is
    // PASS/FAIL (uppercase); c5 uses the C9 enumeration induced /
    // not-triggered / fail; throughput_mibps is printed with two decimals.
    // All %{public} — no key material is ever logged.
    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
        "N1A_RESULT|verdict=%{public}s|c5=%{public}s|throughput_mibps=%{public}.2f",
        probe->ok == 0 ? "PASS" : "FAIL",
        C5MarkerName(probe->criteria[4]),
        probe->throughput_mib_per_sec);

    // r3 C7/C8 evidence short marker (distinct prefix from N1A_RESULT, per
    // the r3 re-review ruling; single line; no pipe characters in values).
    // c7/c8 use pass|fail; fd2 closed|open; fdset is the diff count.
    OH_LOG_Print(LOG_APP, LOG_INFO, kLogDomain, kLogTag,
        "N1A_RES|c7=%{public}s|c8=%{public}s|fd2=%{public}s|fdset=%{public}d",
        probe->criteria[6] == 1 ? "pass" : "fail",
        probe->criteria[7] == 1 ? "pass" : "fail",
        probe->c7_fd2_closed == 1 ? "closed" : "open",
        probe->c7_fdset_diff_count);
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
