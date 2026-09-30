import AbilityConstant from '@ohos.app.ability.AbilityConstant';
import UIAbility from '@ohos.app.ability.UIAbility';
import Want from '@ohos.app.ability.Want';
import fs from '@ohos.file.fs';
import hilog from '@ohos.hilog';
import util from '@ohos.util';
import window from '@ohos.window';

const DOMAIN: number = 0x2900;
const TAG: string = 'NetBirdVpn';
const BUNDLE_NAME: string = 'cn.alfadb.netbird';

// Development-time credential delivery via want parameters.
//
// Why: on device the app sandbox cannot see /data/local/tmp (verified
// 2026-09-13: the VpnExtension read path fell back to filesDir and aborted
// with "missing file under filesDir: netbird-setup-key"). So the launcher
// injects the development config as want parameters instead:
//
//   aa start -b cn.alfadb.netbird -a EntryAbility \
//     --ps netbird_config '<json>' --ps netbird_setup_key '<key>'
//
// These values are written into this app's own filesDir, from where the
// existing credential reader picks them up. DEVELOPMENT ONLY: the files sit in
// plain application storage (not secure storage) and only exist because the
// operator explicitly launched the app with these parameters. Values are never
// logged (only file name + byte count).
const CONFIG_PARAM: string = 'netbird_config';
const SETUP_KEY_PARAM: string = 'netbird_setup_key';
const PROTECT_DEVIATION_PARAM: string = 'protectDeviation';
const CONFIG_FILE: string = 'netbird-device-config.json';
const SETUP_KEY_FILE: string = 'netbird-setup-key';
// Handed to the VpnExtensionAbility through filesDir rather than through its
// want: Index.buildWant() constructs the extension want itself and does not
// forward launcher parameters (device-verified 2026-09-14: the extension
// never saw --ps protectDeviation).
const PROTECT_DEVIATION_FILE: string = 'netbird-protect-deviation';
// N2-H arm private channel (2026-09-18): the non-secret diagnostic arm
// ticket. The `n2h_arm_options` parameter carries ONLY diagnostic window
// options (posctl_port / probe_wait_ms / peer_b2a_probe_id) — never
// setup-key / private-key material, which keep their dedicated parameters
// above. The value is written verbatim into this app's OWN filesDir under the
// SAME basename the VpnExtensionAbility reads: /data/local/tmp is invisible
// to the app sandbox (device 19:49-19:50 — accessSync answered boolean false
// while the shell listed the file), so the shared path is no longer used.
// A launch WITHOUT the parameter deletes a leftover ticket so a normal start
// cannot open the diagnostic window (default: no arm). Payload is never
// logged — file name + byte count on success, error CODE on failure.
const N2H_ARM_OPTIONS_PARAM: string = 'n2h_arm_options';
const N2H_ARM_OPTIONS_FILE: string = 'netbird-n2h-arm';
// N2 T5a「B 断流刺激」private channel (2026-09-28): the SAME non-secret
// want-parameter mechanism as `n2h_arm_options`, with its own payload file. The
// basename must match the extension's `N2_STIMULUS_FILE_NAME`
// (NetBirdN2Stimulus.ets) exactly — the two abilities share filesDir, not
// imports (the same literal-basename convention as the arm ticket above).
// An EXPLICIT parameter writes the one pending stimulus; every launch without
// it deletes a leftover so a normal start cannot fire a stale stimulus. The
// extension consumes (deletes) the file when it applies it, so one launch =
// one stimulus.
const N2_STIMULUS_PARAM: string = 'n2_stimulus';
const N2_STIMULUS_FILE: string = 'netbird-n2-stimulus';
// N2 T3 §3 positive-control private channel (2026-09-28): the SAME
// non-secret want-parameter mechanism as `n2_stimulus`, with its own payload
// file. The basename must match the extension's `N2_CONTROL_FILE_NAME`
// (NetBirdN2ControlRequest.ets) exactly — the two abilities share filesDir,
// not imports (the same literal-basename convention as the arm ticket and the
// stimulus above). ONE launch carries at most ONE pending request: a launch
// with BOTH `n2_stimulus` and `n2_control` writes NEITHER (logged as
// N2_CONTROL_COMBO_REJECTED) and removes both leftovers, and a launch with
// only one of them removes the other's leftover — so a stale request can
// never fire later and the executor's three separate calls (pre-control ->
// production event -> post-control) each carry exactly the request they mean.
// The payload is never logged.
const N2_CONTROL_PARAM: string = 'n2_control';
const N2_CONTROL_FILE: string = 'netbird-n2-control';
// N2 P4「A 列生产事件触发原语」(2026-09-28): the SAME non-secret
// want-parameter mechanism again, with its own payload file. The basename must
// match the extension's `N2_A_EVENT_FILE_NAME` (NetBirdN2AEvent.ets) exactly.
// The combination rule now spans THREE channels: a launch carrying more than
// one of {n2_stimulus, n2_control, n2_a_event} writes NEITHER and removes all
// leftovers (the two-channel refusal keeps its exact pre-existing line; a
// refusal involving the A-event channel adds `|a_event=`), and a launch with
// exactly one writes it while removing the other two leftovers. The payload is
// never logged.
const N2_A_EVENT_PARAM: string = 'n2_a_event';
const N2_A_EVENT_FILE: string = 'netbird-n2-a-event';
// N2 first batch C/D columns (2026-09-29): the SAME non-secret want-parameter
// mechanism again, with its own payload file. The basename must match the
// extension's `N2_CD_FILE_NAME` (NetBirdN2CdPlan.ets) exactly. This is the
// channel the HOST uses while it still has a live link (Wi-Fi) to hand the App
// one switching-sandwich plan (`{"column":"C","kind":"M","rep":1,"id":"…"}`),
// which the App then executes alone during the cellular phase.
const N2_CD_PLAN_PARAM: string = 'n2_cd_plan';
const N2_CD_PLAN_FILE: string = 'netbird-n2-cd-plan';
// N2 first batch C/D columns (2026-09-29): the ring FETCH fallback channel
// (`{"start":0,"count":64}`) — the extension re-emits a bounded window of the
// armed-only app-internal evidence ring on HiLog. The basename must match
// `N2_RING_DUMP_FILE_NAME` (NetBirdN2Ring.ets).
const N2_RING_DUMP_PARAM: string = 'n2_ring_dump';
const N2_RING_DUMP_FILE: string = 'netbird-n2-ring-dump';
// N2 first batch C/D columns (2026-09-29): the EXPLICIT host ACK channel
// (`{"session":"<arm token>","up_to_seq":<durable seq>}`) — the ONLY thing that
// may advance the ring's retrieved watermark, and therefore the only thing that
// can release the oldest segment to rotation. The host sends it only AFTER it
// has persisted the fetched ring bytes and verified the window is complete; the
// extension forwards the document verbatim and the CORE binds it to the current
// arm. The basename must match `N2_RING_ACK_FILE_NAME` (NetBirdN2Ring.ets).
const N2_RING_ACK_PARAM: string = 'n2_ring_ack';
const N2_RING_ACK_FILE: string = 'netbird-n2-ring-ack';

interface BusinessFailure extends Error {
  code?: number;
}

export default class EntryAbility extends UIAbility {
  onCreate(want: Want, _launchParam: AbilityConstant.LaunchParam): void {
    hilog.info(DOMAIN, TAG, 'ENTRY_ONCREATE|bundle=%{public}s', want.bundleName ?? BUNDLE_NAME);
    this.removePreviousStatus();
    this.persistCredentialOverrides(want);
  }

  // A fresh UI process cannot infer liveness from a previous process's file:
  // force-stop leaves its last connected snapshot behind. A live extension
  // publishes again on its next tick.
  private removePreviousStatus(): void {
    const path: string = `${this.context.filesDir}/netbird-status.json`;
    try {
      if (fs.accessSync(path)) {
        fs.unlinkSync(path);
      }
    } catch (error) {
      hilog.warn(DOMAIN, TAG, 'ENTRY_STATUS_REMOVE_FAILED|code=%{public}s',
        String((error as BusinessFailure).code ?? 'none'));
    }
  }

  onNewWant(want: Want, _launchParam: AbilityConstant.LaunchParam): void {
    hilog.info(DOMAIN, TAG, 'ENTRY_ONNEWWANT|bundle=%{public}s', want.bundleName ?? BUNDLE_NAME);
    this.persistCredentialOverrides(want);
  }

  onWindowStageCreate(windowStage: window.WindowStage): void {
    windowStage.loadContent('pages/Index', (error) => {
      if (error.code) {
        hilog.error(DOMAIN, TAG, 'ENTRY_LOAD_REJECTED|code=%{public}d', error.code);
        return;
      }
      hilog.info(DOMAIN, TAG, 'ENTRY_PAGE_READY|bundle=%{public}s', BUNDLE_NAME);
    });
  }

  private persistCredentialOverrides(want: Want): void {
    const params: Record<string, Object> | undefined = want.parameters;
    if (params !== undefined && params !== null) {
      const config: Object | undefined = params[CONFIG_PARAM];
      const setupKey: Object | undefined = params[SETUP_KEY_PARAM];
      const deviation: Object | undefined = params[PROTECT_DEVIATION_PARAM];
      if (typeof config === 'string' && config.length > 0) {
        this.writeFile(CONFIG_FILE, config);
      }
      if (typeof setupKey === 'string' && setupKey.length > 0) {
        this.writeFile(SETUP_KEY_FILE, `${setupKey}\n`);
      }
      if (typeof deviation === 'string' && deviation === 'route-exclusion') {
        this.writeFile(PROTECT_DEVIATION_FILE, `${deviation}\n`);
      } else {
        this.removeProtectDeviation();
      }
    } else {
      this.removeProtectDeviation();
    }
    // N2-H arm private channel: an EXPLICIT non-secret parameter writes the
    // ticket. Whether a launch WITHOUT it clears a leftover is decided further
    // below, AFTER the N2 request parameters are read (N2 T7a item 7: a launch
    // that carries one request but no arm ticket must NOT disarm the running
    // session). The credential writes above are untouched.
    const armOptions: Object | undefined =
      params === undefined || params === null ? undefined : params[N2H_ARM_OPTIONS_PARAM];
    // N2 T5a / T3 / P4 private channels: same discipline as the arm ticket
    // above. ONE launch carries at most ONE pending request (stimulus,
    // positive control or A-column trigger): an explicit non-secret parameter
    // writes it, a launch without it removes that channel's leftover, and a
    // launch carrying MORE THAN ONE writes none of them (the combination is
    // refused, not ordered) while removing every leftover. The payloads are
    // never logged.
    const stimulus: Object | undefined =
      params === undefined || params === null ? undefined : params[N2_STIMULUS_PARAM];
    const control: Object | undefined =
      params === undefined || params === null ? undefined : params[N2_CONTROL_PARAM];
    const aEvent: Object | undefined =
      params === undefined || params === null ? undefined : params[N2_A_EVENT_PARAM];
    const cdPlan: Object | undefined =
      params === undefined || params === null ? undefined : params[N2_CD_PLAN_PARAM];
    const ringDump: Object | undefined =
      params === undefined || params === null ? undefined : params[N2_RING_DUMP_PARAM];
    const ringAck: Object | undefined =
      params === undefined || params === null ? undefined : params[N2_RING_ACK_PARAM];
    const stimulusText: string = typeof stimulus === 'string' ? stimulus : '';
    const controlText: string = typeof control === 'string' ? control : '';
    const aEventText: string = typeof aEvent === 'string' ? aEvent : '';
    const cdPlanText: string = typeof cdPlan === 'string' ? cdPlan : '';
    const ringDumpText: string = typeof ringDump === 'string' ? ringDump : '';
    const ringAckText: string = typeof ringAck === 'string' ? ringAck : '';
    // N2 T7a (item 7) — arm ticket protection. The documented operator flow is
    // "aa start WITH the arm ticket, then one launch per request"; a launch
    // that carries an N2 request parameter but NO arm ticket must therefore
    // KEEP the existing ticket instead of deleting it (deleting it would leave
    // the already-running extension armed but a later extension restart
    // unarmed — the executor's "arm once, then send" shape would silently lose
    // its arm). A launch with NO N2 request parameter keeps the original
    // default-no-arm rule, so a stale ticket still cannot survive an unrelated
    // launch. An explicit ticket always wins and is written verbatim.
    if (typeof armOptions === 'string' && armOptions.length > 0) {
      this.writeArmTicketFile(armOptions);
    } else if (stimulusText.length > 0 || controlText.length > 0 || aEventText.length > 0 ||
      cdPlanText.length > 0 || ringDumpText.length > 0 || ringAckText.length > 0) {
      hilog.info(DOMAIN, TAG,
        'N2H_ARM_TICKET_KEPT|file=%{public}s|note=request-launch-without-arm-ticket|' +
        'diag=true|is_evidence=false',
        N2H_ARM_OPTIONS_FILE);
    } else {
      this.removeArmTicket('default-no-arm');
    }
    // N2 first batch C/D columns (2026-09-29): the one-pending-request-per-launch
    // rule now spans SIX channels. The pre-existing refusals (stimulus+control,
    // and every two-of-three combination involving the A-event channel) keep
    // their exact line shapes; a combination that involves one of the THREE NEW
    // channels (cd_plan, ring_dump, ring_ack) and is not already covered emits
    // the generic `pending=` shape naming exactly the file of each channel that
    // is actually present, in the fixed order stimulus, control, a_event,
    // cd_plan, ring_dump, ring_ack.
    const pendingCount: number = (stimulusText.length > 0 ? 1 : 0) +
      (controlText.length > 0 ? 1 : 0) + (aEventText.length > 0 ? 1 : 0) +
      (cdPlanText.length > 0 ? 1 : 0) + (ringDumpText.length > 0 ? 1 : 0) +
      (ringAckText.length > 0 ? 1 : 0);
    const removeEveryPendingRequest = (note: string): void => {
      this.removeStimulusFile(note);
      this.removeControlFile(note);
      this.removeAEventFile(note);
      this.removeCdPlanFile(note);
      this.removeRingDumpFile(note);
      this.removeRingAckFile(note);
    };
    if (stimulusText.length > 0 && controlText.length > 0) {
      // The pre-existing two-channel refusal keeps its exact line; a THIRD
      // pending request (the A-event trigger) extends the same refusal with
      // `|a_event=` — the combination rule now spans three channels.
      if (aEventText.length > 0) {
        hilog.warn(DOMAIN, TAG,
          'N2_CONTROL_COMBO_REJECTED|stimulus=%{public}s|control=%{public}s|' +
          'a_event=%{public}s|note=one-pending-request-per-launch|diag=true|is_evidence=false',
          N2_STIMULUS_FILE, N2_CONTROL_FILE, N2_A_EVENT_FILE);
      } else {
        hilog.warn(DOMAIN, TAG,
          'N2_CONTROL_COMBO_REJECTED|stimulus=%{public}s|control=%{public}s|' +
          'note=one-pending-request-per-launch|diag=true|is_evidence=false',
          N2_STIMULUS_FILE, N2_CONTROL_FILE);
      }
      this.removeStimulusFile('combo-rejected');
      this.removeControlFile('combo-rejected');
      this.removeAEventFile('combo-rejected');
      this.removeCdPlanFile('combo-rejected');
      this.removeRingDumpFile('combo-rejected');
      this.removeRingAckFile('combo-rejected');
    } else if (stimulusText.length > 0 && aEventText.length > 0) {
      hilog.warn(DOMAIN, TAG,
        'N2_CONTROL_COMBO_REJECTED|stimulus=%{public}s|control=%{public}s|' +
        'a_event=%{public}s|note=one-pending-request-per-launch|diag=true|is_evidence=false',
        N2_STIMULUS_FILE, N2_CONTROL_FILE, N2_A_EVENT_FILE);
      this.removeStimulusFile('combo-rejected');
      this.removeControlFile('combo-rejected');
      this.removeAEventFile('combo-rejected');
      this.removeCdPlanFile('combo-rejected');
      this.removeRingDumpFile('combo-rejected');
      this.removeRingAckFile('combo-rejected');
    } else if (controlText.length > 0 && aEventText.length > 0) {
      hilog.warn(DOMAIN, TAG,
        'N2_CONTROL_COMBO_REJECTED|stimulus=%{public}s|control=%{public}s|' +
        'a_event=%{public}s|note=one-pending-request-per-launch|diag=true|is_evidence=false',
        N2_STIMULUS_FILE, N2_CONTROL_FILE, N2_A_EVENT_FILE);
      this.removeStimulusFile('combo-rejected');
      this.removeControlFile('combo-rejected');
      this.removeAEventFile('combo-rejected');
      this.removeCdPlanFile('combo-rejected');
      this.removeRingDumpFile('combo-rejected');
      this.removeRingAckFile('combo-rejected');
    } else if (pendingCount > 1) {
      // A combination the pre-existing branches do not cover. It always
      // involves `n2_cd_plan`, `n2_ring_dump` or `n2_ring_ack` (those are the
      // only channels left once the three branches above did not match), so the
      // line names exactly the pending channels by file.
      let pending: string = '';
      if (stimulusText.length > 0) { pending += (pending.length > 0 ? ',' : '') + N2_STIMULUS_FILE; }
      if (controlText.length > 0) { pending += (pending.length > 0 ? ',' : '') + N2_CONTROL_FILE; }
      if (aEventText.length > 0) { pending += (pending.length > 0 ? ',' : '') + N2_A_EVENT_FILE; }
      if (cdPlanText.length > 0) { pending += (pending.length > 0 ? ',' : '') + N2_CD_PLAN_FILE; }
      if (ringDumpText.length > 0) { pending += (pending.length > 0 ? ',' : '') + N2_RING_DUMP_FILE; }
      if (ringAckText.length > 0) { pending += (pending.length > 0 ? ',' : '') + N2_RING_ACK_FILE; }
      hilog.warn(DOMAIN, TAG,
        'N2_CONTROL_COMBO_REJECTED|pending=%{public}s|' +
        'note=one-pending-request-per-launch|diag=true|is_evidence=false',
        pending);
      removeEveryPendingRequest('combo-rejected');
    } else if (stimulusText.length > 0) {
      this.writeStimulusFile(stimulusText);
      this.removeControlFile('default-no-control');
      this.removeAEventFile('default-no-a-event');
      this.removeCdPlanFile('default-no-cd-plan');
      this.removeRingDumpFile('default-no-ring-dump');
      this.removeRingAckFile('default-no-ring-ack');
    } else if (controlText.length > 0) {
      this.writeControlFile(controlText);
      this.removeStimulusFile('default-no-stimulus');
      this.removeAEventFile('default-no-a-event');
      this.removeCdPlanFile('default-no-cd-plan');
      this.removeRingDumpFile('default-no-ring-dump');
      this.removeRingAckFile('default-no-ring-ack');
    } else if (aEventText.length > 0) {
      this.writeAEventFile(aEventText);
      this.removeStimulusFile('default-no-stimulus');
      this.removeControlFile('default-no-control');
      this.removeCdPlanFile('default-no-cd-plan');
      this.removeRingDumpFile('default-no-ring-dump');
      this.removeRingAckFile('default-no-ring-ack');
    } else if (cdPlanText.length > 0) {
      this.writeCdPlanFile(cdPlanText);
      this.removeStimulusFile('default-no-stimulus');
      this.removeControlFile('default-no-control');
      this.removeAEventFile('default-no-a-event');
      this.removeRingDumpFile('default-no-ring-dump');
      this.removeRingAckFile('default-no-ring-ack');
    } else if (ringDumpText.length > 0) {
      this.writeRingDumpFile(ringDumpText);
      this.removeStimulusFile('default-no-stimulus');
      this.removeControlFile('default-no-control');
      this.removeAEventFile('default-no-a-event');
      this.removeCdPlanFile('default-no-cd-plan');
      this.removeRingAckFile('default-no-ring-ack');
    } else if (ringAckText.length > 0) {
      this.writeRingAckFile(ringAckText);
      this.removeStimulusFile('default-no-stimulus');
      this.removeControlFile('default-no-control');
      this.removeAEventFile('default-no-a-event');
      this.removeCdPlanFile('default-no-cd-plan');
      this.removeRingDumpFile('default-no-ring-dump');
    } else {
      this.removeStimulusFile('default-no-stimulus');
      this.removeControlFile('default-no-control');
      this.removeAEventFile('default-no-a-event');
      this.removeCdPlanFile('default-no-cd-plan');
      this.removeRingDumpFile('default-no-ring-dump');
      this.removeRingAckFile('default-no-ring-ack');
    }
  }

  private removeProtectDeviation(): void {
    const path: string = `${this.context.filesDir}/${PROTECT_DEVIATION_FILE}`;
    try {
      if (!fs.accessSync(path)) { return; }
      fs.unlinkSync(path);
      hilog.info(DOMAIN, TAG, 'PROTECT_DEVIATION_REMOVED|file=%{public}s', PROTECT_DEVIATION_FILE);
    } catch (error) {
      hilog.error(DOMAIN, TAG, 'PROTECT_DEVIATION_REMOVE_FAILED|file=%{public}s|code=%{public}s',
        PROTECT_DEVIATION_FILE, String((error as BusinessFailure).code ?? 'none'));
    }
  }

  private writeFile(name: string, text: string): void {
    const path: string = `${this.context.filesDir}/${name}`;
    let file: fs.File | undefined = undefined;
    try {
      file = fs.openSync(path, fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
      fs.writeSync(file.fd, text);
      hilog.warn(DOMAIN, TAG,
        'CRED_WANT_OVERRIDE|file=%{public}s|bytes=%{public}d|note=development-only-not-secure-storage',
        name, text.length);
    } catch (error) {
      const message: string = (error as Error).message ?? 'unknown';
      hilog.error(DOMAIN, TAG, 'CRED_WANT_OVERRIDE_FAILED|file=%{public}s|message=%{public}s',
        name, message.slice(0, 80));
    } finally {
      if (file !== undefined) {
        fs.closeSync(file);
      }
    }
  }

  // N2-H arm private channel: the SAME writeFile style as the credentials
  // (open trunc + write + close in this app's own filesDir) with its own
  // markers, and the honesty rule in BOTH failure shapes:
  //   - openSync throws → nothing was created, no ticket, log the error CODE.
  //   - writeSync throws or writes SHORT after a successful open — the open's
  //     TRUNC has already wiped any previous ticket — then the on-disk file is
  //     an empty/partial ticket that the extension would parse into the
  //     §5.1 defaults and ARM. So the failure path REMOVES exactly this one
  //     diagnostic ticket after closing the fd (removeArmTicket below logs the
  //     removal — or its own failure — independently with its error CODE; a
  //     removal failure is never reported as a removal).
  // Completeness: fs.writeSync returns the number of bytes ACTUALLY written
  // and, for a string buffer, those are UTF-8 bytes (SDK contract). The
  // expected count is the payload's real UTF-8 byte length via the platform
  // TextEncoder — never the UTF-16 code-unit `text.length`. The success log
  // reports the same UTF-8 byte count. Payload is never logged.
  private writeArmTicketFile(text: string): void {
    const path: string = `${this.context.filesDir}/${N2H_ARM_OPTIONS_FILE}`;
    let file: fs.File | undefined = undefined;
    let writeFailed: boolean = false;
    let failureCode: string = 'none';
    let expectedBytes: number = 0;
    try {
      file = fs.openSync(path, fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
      const written: number = fs.writeSync(file.fd, text);
      expectedBytes = new util.TextEncoder().encode(text).byteLength;
      if (written !== expectedBytes) {
        writeFailed = true;
        failureCode = `short-write:${written}of${expectedBytes}`;
      }
    } catch (error) {
      writeFailed = true;
      failureCode = String((error as BusinessFailure).code ?? 'none');
    } finally {
      if (file !== undefined) {
        try {
          fs.closeSync(file);
        } catch (closeError) {
          // The ticket's fate is already decided by the write outcome below;
          // a close failure is its own visible event, never swallowed.
          hilog.error(DOMAIN, TAG,
            'N2H_ARM_TICKET_CLOSE_FAILED|file=%{public}s|code=%{public}s',
            N2H_ARM_OPTIONS_FILE, String((closeError as BusinessFailure).code ?? 'none'));
        }
      }
    }
    if (!writeFailed) {
      hilog.warn(DOMAIN, TAG,
        'N2H_ARM_TICKET_WRITTEN|file=%{public}s|bytes=%{public}d|note=diagnostic-only-non-secret',
        N2H_ARM_OPTIONS_FILE, expectedBytes);
      return;
    }
    hilog.error(DOMAIN, TAG,
      'N2H_ARM_TICKET_WRITE_FAILED|file=%{public}s|code=%{public}s|note=no-ticket-no-arm',
      N2H_ARM_OPTIONS_FILE, failureCode);
    this.removeArmTicket('failed-write-cleanup');
  }

  // N2-H arm private channel: delete THE ONE diagnostic-generated ticket file
  // (basename above) — from the default no-arm launch path AND from a failed
  // ticket write (the open's TRUNC would otherwise leave an empty file the
  // extension could arm on). Credential / config / deviation files are NEVER
  // touched here. A missing ticket is the normal quiet case; a failed removal
  // is logged with its error CODE and the note — the surviving (or wiped)
  // file is reported as what it is, never as removed.
  private removeArmTicket(note: string): void {
    const path: string = `${this.context.filesDir}/${N2H_ARM_OPTIONS_FILE}`;
    try {
      if (!fs.accessSync(path)) {
        return;
      }
      fs.unlinkSync(path);
      hilog.info(DOMAIN, TAG,
        'N2H_ARM_TICKET_REMOVED|file=%{public}s|note=%{public}s', N2H_ARM_OPTIONS_FILE, note);
    } catch (error) {
      const code: string = String((error as BusinessFailure).code ?? 'none');
      hilog.error(DOMAIN, TAG,
        'N2H_ARM_TICKET_REMOVE_FAILED|file=%{public}s|code=%{public}s|note=%{public}s',
        N2H_ARM_OPTIONS_FILE, code, note);
    }
  }

  // N2 T5a「B 断流刺激」private channel (2026-09-28). Same write honesty as
  // writeArmTicketFile above: the open's TRUNC already wiped any previous
  // request, so a failed/short WRITE must not leave an empty or partial file
  // that the extension could parse into a stimulus — the failure path removes
  // exactly this one file (logged independently, with its own error code; a
  // removal failure is never reported as a removal). The payload is never
  // logged, and `bytes=` is the real UTF-8 byte count (never the UTF-16
  // code-unit `text.length`).
  private writeStimulusFile(text: string): void {
    const path: string = `${this.context.filesDir}/${N2_STIMULUS_FILE}`;
    let file: fs.File | undefined = undefined;
    let writeFailed: boolean = false;
    let failureCode: string = 'none';
    let expectedBytes: number = 0;
    try {
      file = fs.openSync(path, fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
      const written: number = fs.writeSync(file.fd, text);
      expectedBytes = new util.TextEncoder().encode(text).byteLength;
      if (written !== expectedBytes) {
        writeFailed = true;
        failureCode = `short-write:${written}of${expectedBytes}`;
      }
    } catch (error) {
      writeFailed = true;
      failureCode = String((error as BusinessFailure).code ?? 'none');
    } finally {
      if (file !== undefined) {
        try {
          fs.closeSync(file);
        } catch (closeError) {
          hilog.error(DOMAIN, TAG,
            'N2_STIMULUS_FILE_CLOSE_FAILED|file=%{public}s|code=%{public}s',
            N2_STIMULUS_FILE, String((closeError as BusinessFailure).code ?? 'none'));
        }
      }
    }
    if (!writeFailed) {
      hilog.warn(DOMAIN, TAG,
        'N2_STIMULUS_FILE_WRITTEN|file=%{public}s|bytes=%{public}d|note=diagnostic-only-non-secret',
        N2_STIMULUS_FILE, expectedBytes);
      return;
    }
    hilog.error(DOMAIN, TAG,
      'N2_STIMULUS_FILE_WRITE_FAILED|file=%{public}s|code=%{public}s|note=no-stimulus',
      N2_STIMULUS_FILE, failureCode);
    this.removeStimulusFile('failed-write-cleanup');
  }

  // Delete THE ONE stimulus request file (basename above) — from the default
  // no-stimulus launch path, from the combination refusal, and from a failed
  // write. Credential / config / deviation / arm-ticket / control-request files
  // are NEVER touched here.
  private removeStimulusFile(note: string): void {
    const path: string = `${this.context.filesDir}/${N2_STIMULUS_FILE}`;
    try {
      if (!fs.accessSync(path)) {
        return;
      }
      fs.unlinkSync(path);
      hilog.info(DOMAIN, TAG,
        'N2_STIMULUS_FILE_REMOVED|file=%{public}s|note=%{public}s', N2_STIMULUS_FILE, note);
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'N2_STIMULUS_FILE_REMOVE_FAILED|file=%{public}s|code=%{public}s|note=%{public}s',
        N2_STIMULUS_FILE, String((error as BusinessFailure).code ?? 'none'), note);
    }
  }

  // N2 T3 §3 positive-control private channel (2026-09-28). The SAME write
  // honesty as writeStimulusFile above: the open's TRUNC already wiped any
  // previous request, so a failed/short WRITE must not leave an empty or
  // partial file the extension could parse into a control — the failure path
  // removes exactly this one file (logged independently, with its own error
  // code; a removal failure is never reported as a removal). The payload is
  // never logged, and `bytes=` is the real UTF-8 byte count (never the UTF-16
  // code-unit `text.length`).
  private writeControlFile(text: string): void {
    const path: string = `${this.context.filesDir}/${N2_CONTROL_FILE}`;
    let file: fs.File | undefined = undefined;
    let writeFailed: boolean = false;
    let failureCode: string = 'none';
    let expectedBytes: number = 0;
    try {
      file = fs.openSync(path, fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
      const written: number = fs.writeSync(file.fd, text);
      expectedBytes = new util.TextEncoder().encode(text).byteLength;
      if (written !== expectedBytes) {
        writeFailed = true;
        failureCode = `short-write:${written}of${expectedBytes}`;
      }
    } catch (error) {
      writeFailed = true;
      failureCode = String((error as BusinessFailure).code ?? 'none');
    } finally {
      if (file !== undefined) {
        try {
          fs.closeSync(file);
        } catch (closeError) {
          hilog.error(DOMAIN, TAG,
            'N2_CONTROL_FILE_CLOSE_FAILED|file=%{public}s|code=%{public}s',
            N2_CONTROL_FILE, String((closeError as BusinessFailure).code ?? 'none'));
        }
      }
    }
    if (!writeFailed) {
      hilog.warn(DOMAIN, TAG,
        'N2_CONTROL_FILE_WRITTEN|file=%{public}s|bytes=%{public}d|note=diagnostic-only-non-secret',
        N2_CONTROL_FILE, expectedBytes);
      return;
    }
    hilog.error(DOMAIN, TAG,
      'N2_CONTROL_FILE_WRITE_FAILED|file=%{public}s|code=%{public}s|note=no-control',
      N2_CONTROL_FILE, failureCode);
    this.removeControlFile('failed-write-cleanup');
  }

  // Delete THE ONE positive-control request file (basename above) — from the
  // default no-control launch path, from the combination refusal, and from a
  // failed write. Credential / config / deviation / arm-ticket / stimulus /
  // A-event files are NEVER touched here.
  private removeControlFile(note: string): void {
    const path: string = `${this.context.filesDir}/${N2_CONTROL_FILE}`;
    try {
      if (!fs.accessSync(path)) {
        return;
      }
      fs.unlinkSync(path);
      hilog.info(DOMAIN, TAG,
        'N2_CONTROL_FILE_REMOVED|file=%{public}s|note=%{public}s', N2_CONTROL_FILE, note);
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'N2_CONTROL_FILE_REMOVE_FAILED|file=%{public}s|code=%{public}s|note=%{public}s',
        N2_CONTROL_FILE, String((error as BusinessFailure).code ?? 'none'), note);
    }
  }

  // N2 P4「A 列生产事件触发原语」private channel (2026-09-28). The SAME write
  // honesty as writeStimulusFile / writeControlFile above: the open's TRUNC
  // already wiped any previous request, so a failed/short WRITE must not leave
  // an empty or partial file the extension could parse into a trigger — the
  // failure path removes exactly this one file (logged independently, with its
  // own error code; a removal failure is never reported as a removal). The
  // payload is never logged, and `bytes=` is the real UTF-8 byte count.
  private writeAEventFile(text: string): void {
    const path: string = `${this.context.filesDir}/${N2_A_EVENT_FILE}`;
    let file: fs.File | undefined = undefined;
    let writeFailed: boolean = false;
    let failureCode: string = 'none';
    let expectedBytes: number = 0;
    try {
      file = fs.openSync(path, fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
      const written: number = fs.writeSync(file.fd, text);
      expectedBytes = new util.TextEncoder().encode(text).byteLength;
      if (written !== expectedBytes) {
        writeFailed = true;
        failureCode = `short-write:${written}of${expectedBytes}`;
      }
    } catch (error) {
      writeFailed = true;
      failureCode = String((error as BusinessFailure).code ?? 'none');
    } finally {
      if (file !== undefined) {
        try {
          fs.closeSync(file);
        } catch (closeError) {
          hilog.error(DOMAIN, TAG,
            'N2_A_EVENT_FILE_CLOSE_FAILED|file=%{public}s|code=%{public}s',
            N2_A_EVENT_FILE, String((closeError as BusinessFailure).code ?? 'none'));
        }
      }
    }
    if (!writeFailed) {
      hilog.warn(DOMAIN, TAG,
        'N2_A_EVENT_FILE_WRITTEN|file=%{public}s|bytes=%{public}d|note=diagnostic-only-non-secret',
        N2_A_EVENT_FILE, expectedBytes);
      return;
    }
    hilog.error(DOMAIN, TAG,
      'N2_A_EVENT_FILE_WRITE_FAILED|file=%{public}s|code=%{public}s|note=no-a-event',
      N2_A_EVENT_FILE, failureCode);
    this.removeAEventFile('failed-write-cleanup');
  }

  // Delete THE ONE A-event request file (basename above) — from the default
  // no-a-event launch path, from the combination refusal, and from a failed
  // write. Credential / config / deviation / arm-ticket / stimulus / control
  // files are NEVER touched here.
  private removeAEventFile(note: string): void {
    const path: string = `${this.context.filesDir}/${N2_A_EVENT_FILE}`;
    try {
      if (!fs.accessSync(path)) {
        return;
      }
      fs.unlinkSync(path);
      hilog.info(DOMAIN, TAG,
        'N2_A_EVENT_FILE_REMOVED|file=%{public}s|note=%{public}s', N2_A_EVENT_FILE, note);
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'N2_A_EVENT_FILE_REMOVE_FAILED|file=%{public}s|code=%{public}s|note=%{public}s',
        N2_A_EVENT_FILE, String((error as BusinessFailure).code ?? 'none'), note);
    }
  }

  // N2 first batch C/D columns (2026-09-29): the three NEW request channels
  // (`n2_cd_plan`, `n2_ring_dump`, `n2_ring_ack`). ONE parameterized helper
  // carries the SAME write
  // honesty and removal discipline as the three channels above — open TRUNC +
  // write + close, a failed or SHORT write removes exactly this one file (the
  // open's TRUNC would otherwise leave an empty payload the extension could
  // parse), and the payload is never logged: only the file name, the real
  // UTF-8 byte count and an error CODE. `marker` is the channel's own log
  // marker prefix (`N2_CD_PLAN_FILE` / `N2_RING_DUMP_FILE` / `N2_RING_ACK_FILE`).
  private writeN2RequestFile(marker: string, name: string, note: string, text: string): void {
    const path: string = `${this.context.filesDir}/${name}`;
    let file: fs.File | undefined = undefined;
    let writeFailed: boolean = false;
    let failureCode: string = 'none';
    let expectedBytes: number = 0;
    try {
      file = fs.openSync(path, fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
      const written: number = fs.writeSync(file.fd, text);
      expectedBytes = new util.TextEncoder().encode(text).byteLength;
      if (written !== expectedBytes) {
        writeFailed = true;
        failureCode = `short-write:${written}of${expectedBytes}`;
      }
    } catch (error) {
      writeFailed = true;
      failureCode = String((error as BusinessFailure).code ?? 'none');
    } finally {
      if (file !== undefined) {
        try {
          fs.closeSync(file);
        } catch (closeError) {
          hilog.error(DOMAIN, TAG, marker + '_CLOSE_FAILED|file=%{public}s|code=%{public}s',
            name, String((closeError as BusinessFailure).code ?? 'none'));
        }
      }
    }
    if (!writeFailed) {
      hilog.warn(DOMAIN, TAG,
        marker + '_WRITTEN|file=%{public}s|bytes=%{public}d|note=diagnostic-only-non-secret',
        name, expectedBytes);
      return;
    }
    hilog.error(DOMAIN, TAG, marker + '_WRITE_FAILED|file=%{public}s|code=%{public}s|note=' + note,
      name, failureCode);
    this.removeN2RequestFile(marker, name, 'failed-write-cleanup');
  }

  private removeN2RequestFile(marker: string, name: string, note: string): void {
    const path: string = `${this.context.filesDir}/${name}`;
    try {
      if (!fs.accessSync(path)) {
        return;
      }
      fs.unlinkSync(path);
      hilog.info(DOMAIN, TAG,
        marker + '_REMOVED|file=%{public}s|note=%{public}s', name, note);
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        marker + '_REMOVE_FAILED|file=%{public}s|code=%{public}s|note=%{public}s',
        name, String((error as BusinessFailure).code ?? 'none'), note);
    }
  }

  private writeCdPlanFile(text: string): void {
    this.writeN2RequestFile('N2_CD_PLAN_FILE', N2_CD_PLAN_FILE, 'no-cd-plan', text);
  }

  private removeCdPlanFile(note: string): void {
    this.removeN2RequestFile('N2_CD_PLAN_FILE', N2_CD_PLAN_FILE, note);
  }

  private writeRingDumpFile(text: string): void {
    this.writeN2RequestFile('N2_RING_DUMP_FILE', N2_RING_DUMP_FILE, 'no-ring-dump', text);
  }

  private removeRingDumpFile(note: string): void {
    this.removeN2RequestFile('N2_RING_DUMP_FILE', N2_RING_DUMP_FILE, note);
  }

  private writeRingAckFile(text: string): void {
    this.writeN2RequestFile('N2_RING_ACK_FILE', N2_RING_ACK_FILE, 'no-ring-ack', text);
  }

  private removeRingAckFile(note: string): void {
    this.removeN2RequestFile('N2_RING_ACK_FILE', N2_RING_ACK_FILE, note);
  }
}
