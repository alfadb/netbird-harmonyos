/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// NetBirdSystemEvents — the platform half of the standalone system-event
// watcher: it subscribes to HiAppEvent system events and hands every group to
// the pure projector (NetBirdSystemEventsCore.ets, host-checked by
// client/entry/selfcheck/run-system-events.mts), which owns the whitelist, the
// bounds, the redaction and the dedup window. Nothing here decides a format.
//
// PUBLIC API (for later wiring; nothing imports this yet)
//   start(role: 'ui' | 'vpn', safeSink: (line: string) => void): boolean
//     true  = the watcher is subscribed; a sys_watcher_started line was emitted
//     false = the platform refused/failed; a sys_watcher_unavailable line was
//             emitted instead and the VPN flow is untouched (never a throw)
//   stop(): void   — removes the subscription; safe to call when not running
//   isRunning(): boolean
//
// WHY: the user needs the system's own verdict on why the VPN process died or
// was throttled — kill reason, freeze, resource over-limit — in the persistent
// log, instead of guessing from an abrupt end of the app log.
//
// OFFICIAL FACTS (SDK d.ts, compileSdkVersion 26.0.0):
//   - domain.OS is the system domain; APP_CRASH (11), APP_FREEZE (11),
//     RESOURCE_OVERLIMIT (12), APP_KILLED (20) are the watched event names.
//   - Watcher.onReceive?: (domain: string, appEventGroups: Array<AppEventGroup>) => void
//     AppEventGroup { name: string; appEventInfos: Array<AppEventInfo> }
//     AppEventInfo { domain, name, eventType, params: object }
//   - addWatcher(watcher): AppEventPackageHolder (null on failure, throws
//     BusinessError on a bad watcher); removeWatcher(watcher): void.
//   - APP_KILLED IS DELIVERED ASYNCHRONOUSLY ON THE NEXT LAUNCH: the event
//     describes an exit that already happened. This code cannot reconstruct a
//     kill that happened before start() ran — the log therefore only ever
//     claims what arrived at or after the sys_watcher_started line. A process
//     that is never launched again after a kill has no next launch to deliver
//     it into; subscribing in BOTH processes (ui entry + vpn extension) keeps
//     the chance that whichever one is launched next reports the exit.
//
// SCOPE / NON-GOALS: no permission requests, no background-task or
//   keep-alive flags, no rawheap / dump / snapshot generation, no
//   hiAppEvent.clearData(), no timers (no setInterval/setTimeout anywhere — the
//   SDK pushes events; nothing here polls), no file IO: the whole output path
//   is the caller's sink, which owns persistence and its own redaction.
//
// SINK CONTRACT: the sink must be cheap and must not block; it runs on the SDK
//   callback thread. Any exception it throws is counted and logged at most
//   MAX_INTERNAL_ERROR_LOGS times, never propagated (a failing log sink must
//   not crash the VPN process).
//
// BOUNDED BY CONSTRUCTION: one callback projects at most MAX_GROUPS_PER_CALLBACK
//   groups (the remainder of the array is ignored), each group at most
//   MAX_INFOS_PER_GROUP infos (the remainder is reported as ONE
//   sys_event_dropped line), every line is <= 512 chars, and an exact
//   re-delivery of the same occurrence becomes one sys_event_dup line instead
//   of a new record. The watcher is removed in stop(); a second start() while
//   running is a no-op.

import hiAppEvent from '@ohos.hiviewdfx.hiAppEvent';
import hilog from '@ohos.hilog';
import process from '@ohos.process';
import { DEDUP_WINDOW, MAX_GROUPS_PER_CALLBACK, MAX_INFOS_PER_GROUP, UNAVAILABLE, EventDeduper,
  buildDroppedLine, buildDupLine, buildWatcherLine, projectInfos } from './NetBirdSystemEventsCore';

const DOMAIN: number = 0x2900;
const TAG: string = 'NetBirdVpn';
const MAX_INTERNAL_ERROR_LOGS: number = 8;

// Watcher names: <= 32 chars, [0-9A-Za-z_], start with a letter, end with a
// letter or digit. One name per role keeps the two subscribing processes
// distinguishable in any platform-side view.
const WATCHER_NAME_UI: string = 'NetBirdSysEventsUi';
const WATCHER_NAME_VPN: string = 'NetBirdSysEventsVpn';

export type SysEventRole = 'ui' | 'vpn';
export type SafeSink = (line: string) => void;

// Platform BusinessError as far as this module reads it (one numeric field).
interface BusinessFailure extends Error {
  code?: number;
}

function nowIso(): string {
  return new Date().toISOString();
}

function codeOf(error: Error): string {
  try {
    return String((error as BusinessFailure).code ?? UNAVAILABLE);
  } catch (inner) {
    return 'unreadable';
  }
}

class SystemEventRuntime {
  private running: boolean = false;
  private role: SysEventRole = 'ui';
  private sink: SafeSink | undefined = undefined;
  private watcher: hiAppEvent.Watcher | undefined = undefined;
  private deduper: EventDeduper = new EventDeduper(DEDUP_WINDOW);
  private internalErrors: number = 0;

  start(role: SysEventRole, safeSink: SafeSink): boolean {
    try {
      if (this.running) {
        return true; // idempotent: a second start() in the same process is ignored
      }
      this.role = role;
      this.sink = safeSink;
      this.deduper = new EventDeduper(DEDUP_WINDOW);
      const watcher: hiAppEvent.Watcher = {
        name: role === 'vpn' ? WATCHER_NAME_VPN : WATCHER_NAME_UI,
        appEventFilters: [{
          domain: hiAppEvent.domain.OS,
          names: [hiAppEvent.event.APP_CRASH, hiAppEvent.event.APP_FREEZE,
            hiAppEvent.event.RESOURCE_OVERLIMIT, hiAppEvent.event.APP_KILLED]
        }],
        onReceive: (domain: string, appEventGroups: Array<hiAppEvent.AppEventGroup>): void => {
          this.onReceive(domain, appEventGroups);
        }
      };
      let holder: hiAppEvent.AppEventPackageHolder | null = null;
      try {
        holder = hiAppEvent.addWatcher(watcher);
      } catch (error) {
        // Unsupported build / rejected watcher: report it to the sink and leave
        // the VPN flow alone.
        this.reportUnavailable('addWatcher', error as Error);
        return false;
      }
      if (holder === null || holder === undefined) {
        this.reportUnavailable('null-holder', undefined);
        return false;
      }
      this.watcher = watcher;
      this.running = true;
      // The subscription boundary: nothing before this line is claimed.
      this.emit(buildWatcherLine(nowIso(), process.pid, this.role, 'started', UNAVAILABLE));
      return true;
    } catch (error) {
      this.noteInternalError('start', error as Error);
      this.running = false;
      this.watcher = undefined;
      this.sink = undefined;
      return false;
    }
  }

  stop(): void {
    try {
      if (!this.running) {
        this.sink = undefined; // idempotent: nothing to remove
        return;
      }
      this.running = false;
      const watcher: hiAppEvent.Watcher | undefined = this.watcher;
      this.watcher = undefined;
      if (watcher !== undefined) {
        try {
          hiAppEvent.removeWatcher(watcher);
        } catch (error) {
          this.noteInternalError('removeWatcher', error as Error);
        }
      }
      // The window dies with the subscription: report what it suppressed, once.
      const suppressed: number = this.deduper.takeSuppressed();
      if (suppressed > 0) {
        this.emit(buildDupLine(nowIso(), process.pid, this.role, suppressed));
      }
      this.emit(buildWatcherLine(nowIso(), process.pid, this.role, 'stopped', UNAVAILABLE));
      this.sink = undefined;
    } catch (error) {
      this.noteInternalError('stop', error as Error);
      this.sink = undefined;
    }
  }

  isRunning(): boolean {
    return this.running;
  }

  private onReceive(domain: string, groups: Array<hiAppEvent.AppEventGroup>): void {
    try {
      if (!this.running || this.sink === undefined) {
        return;
      }
      if (domain !== hiAppEvent.domain.OS) {
        return; // the filter scopes this; a foreign domain is ignored, not logged
      }
      const ts: string = nowIso();
      const pid: number = process.pid;
      const groupCount: number = groups.length < MAX_GROUPS_PER_CALLBACK ? groups.length : MAX_GROUPS_PER_CALLBACK;
      for (let i = 0; i < groupCount; i++) {
        const group: hiAppEvent.AppEventGroup = groups[i];
        const infos: Array<hiAppEvent.AppEventInfo> = group.appEventInfos;
        const paramsList: Array<Object> = [];
        const infoCount: number = infos.length < MAX_INFOS_PER_GROUP ? infos.length : MAX_INFOS_PER_GROUP;
        for (let j = 0; j < infoCount; j++) {
          paramsList.push(infos[j].params);
        }
        // infos.length travels separately so the dropped remainder is honest.
        const lines: string[] = projectInfos(ts, pid, this.role, group.name, paramsList, infos.length, this.deduper);
        for (let k = 0; k < lines.length; k++) {
          this.emit(lines[k]);
        }
      }
      const droppedGroups: number = groups.length - groupCount;
      if (droppedGroups > 0) {
        // Groups beyond the per-callback bound are reported, never silently lost.
        this.emit(buildDroppedLine(ts, pid, this.role, 'APP_EVENT_GROUPS', droppedGroups));
      }
    } catch (error) {
      // A malformed callback must never escape into the SDK / VPN process.
      this.noteInternalError('onReceive', error as Error);
    }
  }

  private reportUnavailable(stage: string, error: Error | undefined): void {
    this.noteInternalError(stage, error);
    const code: string = error === undefined ? UNAVAILABLE : codeOf(error);
    this.emit(buildWatcherLine(nowIso(), process.pid, this.role, 'unavailable', code));
    this.sink = undefined;
  }

  private emit(line: string): void {
    const sink: SafeSink | undefined = this.sink;
    if (sink === undefined) {
      return;
    }
    try {
      sink(line);
    } catch (error) {
      this.noteInternalError('sink', error as Error);
    }
  }

  private noteInternalError(stage: string, error: Error | undefined): void {
    this.internalErrors += 1;
    if (this.internalErrors > MAX_INTERNAL_ERROR_LOGS) {
      return; // keep the counter, stop the log flood
    }
    let code: string = UNAVAILABLE;
    if (error !== undefined) {
      code = codeOf(error);
    }
    try {
      hilog.warn(DOMAIN, TAG, 'SYS_EVENTS_ERROR|stage=%{public}s|code=%{public}s|total=%{public}d',
        stage, code, this.internalErrors);
    } catch (inner) {
      // hilog itself failing is not actionable here.
    }
  }
}

const runtime: SystemEventRuntime = new SystemEventRuntime();

// Wiring (NOT wired yet; the main agent integrates later): start('ui', sink)
// from the UI ability's create callback; start('vpn', sink) from the VPN
// extension's create callback with the persistent phone_log append supplied as
// sink; stop() from the ability/extension DESTROY callback only. The sink must
// be cheap and must not block.
export function start(role: SysEventRole, safeSink: SafeSink): boolean {
  return runtime.start(role, safeSink);
}

export function stop(): void {
  runtime.stop();
}

export function isRunning(): boolean {
  return runtime.isRunning();
}
