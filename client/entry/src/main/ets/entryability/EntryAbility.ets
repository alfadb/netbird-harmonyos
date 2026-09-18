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

interface BusinessFailure extends Error {
  code?: number;
}

export default class EntryAbility extends UIAbility {
  onCreate(want: Want, _launchParam: AbilityConstant.LaunchParam): void {
    hilog.info(DOMAIN, TAG, 'ENTRY_ONCREATE|bundle=%{public}s', want.bundleName ?? BUNDLE_NAME);
    this.persistCredentialOverrides(want);
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
      if (typeof deviation === 'string' && deviation.length > 0) {
        this.writeFile(PROTECT_DEVIATION_FILE, `${deviation}\n`);
      }
    }
    // N2-H arm private channel: an EXPLICIT non-secret parameter writes the
    // ticket; every launch without it clears a leftover ticket from a
    // previous diagnostic round (default: no arm). The credential writes
    // above are untouched.
    const armOptions: Object | undefined =
      params === undefined || params === null ? undefined : params[N2H_ARM_OPTIONS_PARAM];
    if (typeof armOptions === 'string' && armOptions.length > 0) {
      this.writeArmTicketFile(armOptions);
    } else {
      this.removeArmTicket('default-no-arm');
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
}
