import AbilityConstant from '@ohos.app.ability.AbilityConstant';
import UIAbility from '@ohos.app.ability.UIAbility';
import Want from '@ohos.app.ability.Want';
import fs from '@ohos.file.fs';
import hilog from '@ohos.hilog';
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
    if (params === undefined || params === null) {
      return;
    }
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
}
