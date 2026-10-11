/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// 2026-10-10 后台保活（0.0.19 实验版，方案 A）—— the ONLY module in this app
// that talks to backgroundTaskManager / notificationManager / wantAgent.
//
// WHAT THIS FILE IS: the platform adapter for ONE borrowed DATA_TRANSFER
// continuous task held by the UIAbility. The DECISION (when to hold it, what a
// platform verdict means, how long to back off) is the pure
// NetBirdKeepAlivePolicy module; this file only performs the platform calls and
// reports what the platform answered. Keeping the two apart is what makes the
// state machine host-verifiable without a device.
//
// WHY THE UIABILITY HOLDS IT: a continuous task can only be requested by a
// UIAbility, and once granted the platform does not suspend ANY process of the
// app — which is the only keep-alive instrument a third-party app has here
// (there is no VPN task type). The VPN extension process is the one that keeps
// being reclaimed, so this is the point of the whole feature.
//
// THE LIVE-VIEW RULE: for DATA_TRANSFER the platform requires progress updates
// to be published as a 实况窗 (live-view) notification, and cancels the task
// when the progress has not been updated for more than 10 minutes. The
// notification the platform creates for the task is identified by the
// `notificationId` returned by startBackgroundRunning(context, bgModes, agent)
// — the numeric overload returns void and CANNOT supply it, and
// updateBackgroundRunning is documented as unusable for DATA_TRANSFER, so the
// task is started with the array overload and every update is a
// notificationManager.publish() with that same id.
//
// Adapter's answer — 0.0.20 adds refreshKeepAliveText(): the page republishes the
// live view with a caller-supplied body while the tunnel is inactive but the
// reconnect grace still holds the task (the policy's "正在重新连接…").
//
// CLAIM BEFORE CREATE: getAllContinuousTasks() is consulted before every
// request. A DATA_TRANSFER task that already exists belongs to this app (this
// process may have restarted while the system kept the task) and is ADOPTED
// instead of being requested a second time — the platform treats repeated
// requests as abuse, and adopting also gives us the notification id the task
// already has.
//
// FAIL-SOFT, ALWAYS: this module must never be able to break the VPN's own
// start/stop path. Every platform call is wrapped; a failure is one
// `ui_bg_task` diagnostic record (a repeated one is recorded once per process)
// and the caller gets false. The diagnostics writer never throws and no
// credential, URL or document text is ever passed to it.
//
// THE CALLBACKS: continuousTaskCancel events are BUFFERED for the page
// (takeKeepAliveCancels) — the pure policy owns their meaning, and the adapter
// must not keep a second copy of that state. A cancel for a task id that is not
// ours is reported as skipped and dropped. suspend/active change no keep-alive
// decision; 0.0.21 records them with every field the platform gives (see
// recordTaskCallback) and, because an activation means THIS PROCESS just started
// running again after a platform suspension, forwards that one fact to the page
// through notifyKeepAliveResume() so it re-reads the tunnel status on the spot.
// Our OWN stopBackgroundRunning must not be mistaken for a platform
// cancellation, hence the `stopping` latch.

import backgroundTaskManager from '@ohos.resourceschedule.backgroundTaskManager';
import notificationManager from '@ohos.notificationManager';
import wantAgent from '@ohos.app.ability.wantAgent';
import Want from '@ohos.app.ability.Want';
import common from '@ohos.app.ability.common';
import hilog from '@ohos.hilog';
import { event } from './NetBirdDiagnostics';
import type { DiagFields } from './NetBirdDiagnostics';
import { KEEP_ALIVE_NOTIFICATION_TITLE, KEEP_ALIVE_RESUME_ACTIVE,
  keepAliveNotificationText, sanitizeSuspendMessage } from './NetBirdKeepAlivePolicy';
import type { KeepAliveCancelEvent, KeepAliveStatus } from './NetBirdKeepAlivePolicy';

const DOMAIN: number = 0x2900;
const TAG: string = 'NetBirdVpn';
const BUNDLE_NAME: string = 'cn.alfadb.netbird';
const ENTRY_ABILITY_NAME: string = 'EntryAbility';
// The borrowed continuous-task type (module.json5 backgroundModes must declare
// the same literal).
const KEEP_ALIVE_BG_MODE: string = 'dataTransfer';
const KEEP_ALIVE_WANT_REQUEST_CODE: number = 1001;
// The live-view template the system documents for a progress-carrying
// notification. `progressValue` stays 0: a tunnel has no completion percentage
// and none is invented.
const KEEP_ALIVE_TEMPLATE_NAME: string = 'downloadTemplate';
const LIVE_VIEW_TYPE_CODE: number = 8;
// Bound on the buffered platform cancellations: the policy stops asking after
// five of them, so anything past this is a platform storm, not information.
const MAX_BUFFERED_CANCELS: number = 8;
// The revision the ability uses on its way out: no session is left, so an
// in-flight creation belongs to nobody and is withdrawn instead of adopted.
export const KEEP_ALIVE_REVISION_SHUTDOWN: number = -1;

// The ONE record name. Lowercase, not an internal runtime name, and every field
// it carries ('stage' / 'result' / 'reason' / 'count') is on the public key set.
export const KEEP_ALIVE_EVENT_NAME: string = 'ui_bg_task';
export const KEEP_ALIVE_STAGE_START: string = 'start';
export const KEEP_ALIVE_STAGE_REFRESH: string = 'refresh';
export const KEEP_ALIVE_STAGE_STOP: string = 'stop';
export const KEEP_ALIVE_STAGE_CANCEL: string = 'cancel';
export const KEEP_ALIVE_STAGE_SUSPEND: string = 'suspend';
export const KEEP_ALIVE_STAGE_ACTIVE: string = 'active';
export const KEEP_ALIVE_STAGE_CLAIM: string = 'claim';
export const KEEP_ALIVE_RESULT_OK: string = 'ok';
export const KEEP_ALIVE_RESULT_ERROR: string = 'error';
export const KEEP_ALIVE_RESULT_SKIPPED: string = 'skipped';

// 0.0.21（挂起长窗口后的恢复复查）：本模块多了一件与平台调用无关的小事——把"这个进程
// 又开始跑了"递给注册者（页面）。两条来源合流到这一条通道：平台说长时任务重新活跃
// （continuousTaskActive，或挂起回调里 suspendState=false），以及 UIAbility 的
// onForeground。0.0.20 的真机数据证明进程可以被整段挂起 7362 s，恢复后的第一件事必须
// 是重读隧道状态，而不是等页面自己下一拍。这里只转发一个信号：不改任何裁决，不碰平台
// 状态，也不持有页面的任何东西。
export type KeepAliveResumeListener = (reason: string) => void;

let resumeListener: KeepAliveResumeListener | undefined = undefined;

// The page owns the ONE listener (it is the only thing that reads status
// documents); `undefined` removes it when the page goes away.
export function setKeepAliveResumeListener(listener: KeepAliveResumeListener | undefined): void {
  resumeListener = listener;
}

// 0.0.21: fire the resume signal. Fail-soft by construction: this runs inside a
// platform callback (or an ability lifecycle callback), and a throwing listener
// must never escape into either of them.
export function notifyKeepAliveResume(reason: string): void {
  const listener: KeepAliveResumeListener | undefined = resumeListener;
  if (listener === undefined) {
    return;
  }
  try {
    listener(reason);
  } catch (error) {
    hilog.warn(DOMAIN, TAG, 'UI_BG_TASK_RESUME_LISTENER_FAILED|code=%{public}s',
      failureToken(error as Error));
  }
}

interface BusinessFailure extends Error {
  code?: number;
}

interface KeepAliveServiceState {
  context: common.UIAbilityContext | undefined;
  hasTask: boolean;
  notificationId: number;
  continuousTaskId: number;
  startInFlight: boolean;
  // Our own stop is in flight: the platform's cancel event for it is not a
  // verdict and must not be buffered for the policy.
  stopping: boolean;
  // The policy state's revision as last seen. A start answer that arrives after
  // the page moved on is withdrawn instead of being adopted.
  currentRevision: number;
  cancels: KeepAliveCancelEvent[];
  cancelCount: number;
  registered: boolean;
  // "Same error, one record per process": the 2 s tick must not repeat itself.
  reported: Set<string>;
}

const service: KeepAliveServiceState = {
  context: undefined,
  hasTask: false,
  notificationId: -1,
  continuousTaskId: -1,
  startInFlight: false,
  stopping: false,
  currentRevision: -1,
  cancels: [],
  cancelCount: 0,
  registered: false,
  reported: new Set<string>()
};

function failureCode(error: Error): number {
  try {
    const code: number | undefined = (error as BusinessFailure).code;
    return typeof code === 'number' && Number.isFinite(code) ? code : 0;
  } catch (ignored) {
    return 0;
  }
}

// The `reason` of a failed platform call: the platform's own numeric cause when
// it gave one, an honest closed token when it did not.
function failureToken(error: Error): string {
  const code: number = failureCode(error);
  return code === 0 ? 'error' : String(code);
}

// The ONE diagnostic exit. `ok` records are the feature's evidence trail (they
// are already rate-limited by the decision machine); every other result writes
// at most one record per (stage, result, reason) per process.
function recordTask(stage: string, result: string, reason: string, count: number): void {
  const key: string = `${stage}:${result}:${reason}`;
  if (result !== KEEP_ALIVE_RESULT_OK) {
    if (service.reported.has(key)) {
      return;
    }
    service.reported.add(key);
  }
  const fields: DiagFields = {};
  fields['stage'] = stage;
  fields['result'] = result;
  fields['reason'] = reason;
  fields['count'] = count;
  event(KEEP_ALIVE_EVENT_NAME, fields);
}

function isOurTask(id: number): boolean {
  // Before the platform has answered a creation we do not know our task id yet;
  // an event arriving then is still ours (this app holds at most one).
  return !service.hasTask || service.continuousTaskId < 0 || id === service.continuousTaskId;
}

function onContinuousTaskCancel(info: backgroundTaskManager.ContinuousTaskCancelInfo): void {
  if (service.stopping) {
    return;
  }
  if (!isOurTask(info.id)) {
    recordTask(KEEP_ALIVE_STAGE_CANCEL, KEEP_ALIVE_RESULT_SKIPPED, 'foreign-task', 0);
    return;
  }
  const detailed: number = info.detailedReason === undefined ? 0 : info.detailedReason;
  service.hasTask = false;
  service.notificationId = -1;
  service.continuousTaskId = -1;
  service.startInFlight = false;
  service.cancelCount = service.cancelCount + 1;
  // The meaning of this event belongs to the pure policy: buffer it and let the
  // page fold it into the ONE policy state on its next tick.
  if (service.cancels.length >= MAX_BUFFERED_CANCELS) {
    service.cancels.shift();
  }
  const buffered: KeepAliveCancelEvent = { reason: info.reason, detailedReason: detailed };
  service.cancels.push(buffered);
  recordTask(KEEP_ALIVE_STAGE_CANCEL, KEEP_ALIVE_RESULT_OK,
    detailed > 0 ? `${info.reason}/${detailed}` : String(info.reason), service.cancelCount);
  hilog.info(DOMAIN, TAG, 'UI_BG_TASK_CANCEL|id=%{public}d|reason=%{public}d|detailed=%{public}d',
    info.id, info.reason, detailed);
}

// 0.0.21: the suspend/active callback's own record. Same record name and the same
// public key set as every other `ui_bg_task` line — this one merely fills in more
// of it, because the 0.0.20 device record (`stage=suspend reason=-1`) did not say
// what the platform actually meant:
//   stage  suspend | active — the callback's own `suspendState` boolean
//   state  1 | 0            — the SAME boolean as a number, so a reader never has
//                             to re-derive it from the stage token
//   reason the platform's `suspendReason` number verbatim (or `na` when the
//          platform delivered none); an unknown number is recorded as-is, never
//          mapped onto a token and never guessed at
//   count  the platform's `continuousTaskId` — deliberately NOT the verdict
//          counter: this event is not a verdict and pays nothing
//   bytes  the `suspendMessage` length in characters (0 = no message delivered);
//          the message's TEXT never enters a record (free-form platform text),
//          it goes to hilog at W level only, through the pure
//          `sanitizeSuspendMessage` (printable ASCII, `<tok>` for a 16+ char
//          token-shaped run, 120 chars max)
function recordTaskCallback(stage: string, reason: string, taskId: number,
  suspended: boolean, messageChars: number): void {
  const fields: DiagFields = {};
  fields['stage'] = stage;
  fields['result'] = KEEP_ALIVE_RESULT_OK;
  fields['state'] = suspended ? 1 : 0;
  fields['reason'] = reason;
  fields['count'] = taskId;
  fields['bytes'] = messageChars;
  event(KEEP_ALIVE_EVENT_NAME, fields);
}

function onContinuousTaskSuspend(info: backgroundTaskManager.ContinuousTaskSuspendInfo): void {
  const taskId: number = info.continuousTaskId;
  if (!isOurTask(taskId)) {
    return;
  }
  // The three fields are read by their DOCUMENTED names (API 26
  // @ohos.resourceschedule.backgroundTaskManager.d.ts):
  //   continuousTaskId  number
  //   suspendState      boolean  (true = suspended, false = activated again)
  //   suspendReason     ContinuousTaskSuspendReason (4..19 on this SDK)
  // plus the API 26 optional `suspendMessage` ({message, reason}). A reason the
  // enum does not contain (a device delivered -1 in the 0.0.20 run) is the
  // platform's own number and is recorded exactly as delivered.
  const suspended: boolean = info.suspendState === true;
  const reason: number = info.suspendReason;
  const reasonText: string = Number.isFinite(reason) ? String(reason) : 'na';
  // The message is free-form platform text and is NEVER copied into a record;
  // the record keeps its raw length, the text goes to hilog at W level (the
  // device log keeps it there) — through the pure sanitizer, so the line carries
  // printable ASCII only, folds a 16+ char token-shaped run into `<tok>` and is
  // bounded to 120 chars. The raw original is not logged anywhere.
  let messageChars: number = 0;
  const message: backgroundTaskManager.SuspendMessage | undefined = info.suspendMessage;
  if (message !== undefined && message !== null) {
    const text: string = message.message;
    if (typeof text === 'string' && text.length > 0) {
      messageChars = text.length;
      hilog.warn(DOMAIN, TAG, 'UI_BG_TASK_SUSPEND_MSG|id=%{public}d|chars=%{public}d|text=%{public}s',
        taskId, messageChars, sanitizeSuspendMessage(text));
    }
  }
  // Diagnostics only: a suspension changes no decision (the task still exists,
  // and the page's own status document is what says whether the tunnel is up).
  recordTaskCallback(suspended ? KEEP_ALIVE_STAGE_SUSPEND : KEEP_ALIVE_STAGE_ACTIVE,
    reasonText, taskId, suspended, messageChars);
  hilog.info(DOMAIN, TAG,
    'UI_BG_TASK_SUSPEND|id=%{public}d|state=%{public}d|reason=%{public}s|msgChars=%{public}d',
    taskId, suspended ? 1 : 0, reasonText, messageChars);
  if (!suspended) {
    // suspendState=false is the platform saying this task runs again, i.e. the
    // process has just come back: the page re-reads the tunnel document NOW
    // (0.0.21) instead of waiting for its next 2 s tick.
    notifyKeepAliveResume(KEEP_ALIVE_RESUME_ACTIVE);
  }
}

function onContinuousTaskActive(info: backgroundTaskManager.ContinuousTaskActiveInfo): void {
  const taskId: number = info.id;
  if (!isOurTask(taskId)) {
    return;
  }
  // ContinuousTaskActiveInfo carries ONE field (`id`, API 20+): the record says
  // exactly that, plus `state=0` (the same "not suspended" fact as a number).
  recordTaskCallback(KEEP_ALIVE_STAGE_ACTIVE, 'platform-active', taskId, false, 0);
  hilog.info(DOMAIN, TAG, 'UI_BG_TASK_ACTIVE|id=%{public}d', taskId);
  // 0.0.21: this is the platform's own "the task is running again" — the
  // strongest resume signal there is, so the page re-reads on the spot.
  notifyKeepAliveResume(KEEP_ALIVE_RESUME_ACTIVE);
}

// Ask for the notification permission ONLY when it is actually missing: a
// continuous-task notification the user never allowed would silently leave the
// task without its live view (and the task would then be cancelled for not
// updating). Best-effort: a refusal changes nothing here.
function ensureNotificationEnabled(context: common.UIAbilityContext): void {
  try {
    notificationManager.isNotificationEnabled().then((enabled: boolean): void => {
      if (enabled) {
        return;
      }
      notificationManager.requestEnableNotification(context).then((): void => {
        hilog.info(DOMAIN, TAG, 'UI_BG_TASK_NOTIFICATION_ENABLED');
      }).catch((error: Error): void => {
        hilog.warn(DOMAIN, TAG, 'UI_BG_TASK_NOTIFICATION_REFUSED|code=%{public}s',
          failureToken(error));
      });
    }).catch((error: Error): void => {
      hilog.warn(DOMAIN, TAG, 'UI_BG_TASK_NOTIFICATION_CHECK_FAILED|code=%{public}s',
        failureToken(error));
    });
  } catch (error) {
    hilog.warn(DOMAIN, TAG, 'UI_BG_TASK_NOTIFICATION_CHECK_FAILED|code=%{public}s',
      failureToken(error as Error));
  }
}

// Registered once per UI process, before the page can ask for anything.
export function registerKeepAlive(context: common.UIAbilityContext): void {
  service.context = context;
  if (service.registered) {
    return;
  }
  try {
    backgroundTaskManager.on('continuousTaskCancel', onContinuousTaskCancel);
    backgroundTaskManager.on('continuousTaskSuspend', onContinuousTaskSuspend);
    backgroundTaskManager.on('continuousTaskActive', onContinuousTaskActive);
    service.registered = true;
  } catch (error) {
    recordTask(KEEP_ALIVE_STAGE_START, KEEP_ALIVE_RESULT_ERROR, failureToken(error as Error), 0);
  }
  ensureNotificationEnabled(context);
}

export function unregisterKeepAlive(): void {
  if (service.registered) {
    try {
      backgroundTaskManager.off('continuousTaskCancel', onContinuousTaskCancel);
      backgroundTaskManager.off('continuousTaskSuspend', onContinuousTaskSuspend);
      backgroundTaskManager.off('continuousTaskActive', onContinuousTaskActive);
    } catch (error) {
      // Ability destruction releases this process's listeners anyway.
    }
    service.registered = false;
  }
  service.context = undefined;
  service.cancels = [];
}

// The page mirrors "does a task exist right now" from here before every
// decision, so the adapter stays the owner of that fact.
export function keepAliveTaskPresent(): boolean {
  return service.hasTask;
}

// Drain the buffered platform cancellations (consume-once).
export function takeKeepAliveCancels(): KeepAliveCancelEvent[] {
  const buffered: KeepAliveCancelEvent[] = service.cancels;
  service.cancels = [];
  return buffered;
}

// Adopt a DATA_TRANSFER task this app already holds, if there is one.
async function claimExistingTask(context: common.UIAbilityContext): Promise<boolean> {
  try {
    const tasks: backgroundTaskManager.ContinuousTaskInfo[] =
      await backgroundTaskManager.getAllContinuousTasks(context, true);
    for (let i = 0; i < tasks.length; i++) {
      const task: backgroundTaskManager.ContinuousTaskInfo = tasks[i];
      if (!isDataTransferTask(task)) {
        continue;
      }
      service.hasTask = true;
      service.notificationId = task.notificationId;
      service.continuousTaskId = task.continuousTaskId;
      hilog.info(DOMAIN, TAG, 'UI_BG_TASK_CLAIMED|id=%{public}d|notification=%{public}d',
        task.continuousTaskId, task.notificationId);
      return true;
    }
    return false;
  } catch (error) {
    recordTask(KEEP_ALIVE_STAGE_CLAIM, KEEP_ALIVE_RESULT_ERROR, failureToken(error as Error), 0);
    return false;
  }
}

// A task belongs to this module when it is a DATA_TRANSFER task of THIS bundle.
// The background-mode literal is compared case-insensitively (it is the same
// string the request used) and the bundle name is only honoured when the
// platform delivered one.
function isDataTransferTask(task: backgroundTaskManager.ContinuousTaskInfo): boolean {
  const bundleName: string | undefined = task.bundleName;
  if (bundleName !== undefined && bundleName !== null && bundleName.length > 0 &&
    bundleName !== BUNDLE_NAME) {
    return false;
  }
  const modes: string[] | undefined = task.backgroundModes;
  if (modes === undefined || modes === null) {
    return false;
  }
  for (let i = 0; i < modes.length; i++) {
    if (modes[i].toLowerCase() === KEEP_ALIVE_BG_MODE.toLowerCase()) {
      return true;
    }
  }
  return false;
}

// Publish ONE live-view update with the task's own notification id. Returns true
// only when the platform accepted it. `body` is built by the CALLER: the page's
// tick passes the tunnel's own traffic (keepAliveNotificationText over the
// document it already read) or, inside the 0.0.20 reconnect grace, the policy's
// "正在重新连接…" — this adapter owns no text of its own.
async function publishLiveView(context: common.UIAbilityContext, body: string,
  reason: string): Promise<boolean> {
  if (service.notificationId < 0) {
    recordTask(KEEP_ALIVE_STAGE_REFRESH, KEEP_ALIVE_RESULT_SKIPPED, 'no-notification-id', 0);
    return false;
  }
  const liveView: notificationManager.NotificationSystemLiveViewContent = {
    typeCode: LIVE_VIEW_TYPE_CODE,
    title: KEEP_ALIVE_NOTIFICATION_TITLE,
    text: body
  };
  const content: notificationManager.NotificationContent = {
    notificationContentType: notificationManager.ContentType.NOTIFICATION_CONTENT_SYSTEM_LIVE_VIEW,
    systemLiveView: liveView
  };
  const templateData: Record<string, Object> = {
    'title': KEEP_ALIVE_NOTIFICATION_TITLE,
    'fileName': body,
    'progressValue': 0
  };
  const template: notificationManager.NotificationTemplate = {
    name: KEEP_ALIVE_TEMPLATE_NAME,
    data: templateData
  };
  const request: notificationManager.NotificationRequest = {
    id: service.notificationId,
    notificationSlotType: notificationManager.SlotType.LIVE_VIEW,
    isAlertOnce: true,
    content: content,
    template: template
  };
  try {
    await notificationManager.publish(request);
    recordTask(KEEP_ALIVE_STAGE_REFRESH, KEEP_ALIVE_RESULT_OK, reason, service.cancelCount);
    return true;
  } catch (error) {
    recordTask(KEEP_ALIVE_STAGE_REFRESH, KEEP_ALIVE_RESULT_ERROR, failureToken(error as Error),
      service.cancelCount);
    return false;
  }
}

// One request for the task. The page has already decided; this only carries it
// out. `revision` is the policy state's session identity and `reason` its
// decision token (the record's `reason`). The tunnel's status document is
// passed in so the FIRST live-view update is published in the same call — the
// page never re-reads anything for it.
export async function startKeepAlive(context: common.UIAbilityContext, revision: number,
  reason: string, status: KeepAliveStatus): Promise<boolean> {
  service.context = context;
  service.currentRevision = revision;
  if (service.hasTask || service.startInFlight) {
    // Already held (or being created): nothing to do, and nothing to report.
    return service.hasTask;
  }
  if (await claimExistingTask(context)) {
    recordTask(KEEP_ALIVE_STAGE_CLAIM, KEEP_ALIVE_RESULT_OK, reason, service.cancelCount);
    await publishLiveView(context, keepAliveNotificationText(status), reason);
    return true;
  }
  service.startInFlight = true;
  const startedRevision: number = revision;
  try {
    const want: Want = {
      bundleName: BUNDLE_NAME,
      abilityName: ENTRY_ABILITY_NAME
    };
    const wants: Want[] = [want];
    const flags: wantAgent.WantAgentFlags[] = [wantAgent.WantAgentFlags.UPDATE_PRESENT_FLAG];
    const info: wantAgent.WantAgentInfo = {
      wants: wants,
      actionType: wantAgent.OperationType.START_ABILITY,
      requestCode: KEEP_ALIVE_WANT_REQUEST_CODE,
      actionFlags: flags
    };
    const agent: object = await wantAgent.getWantAgent(info);
    // The array overload is the ONLY one that returns the system-owned
    // notification id the live-view updates need.
    const task: backgroundTaskManager.ContinuousTaskNotification =
      await backgroundTaskManager.startBackgroundRunning(context, [KEEP_ALIVE_BG_MODE], agent);
    service.hasTask = true;
    service.notificationId = task.notificationId;
    service.continuousTaskId = task.continuousTaskId === undefined ? -1 : task.continuousTaskId;
    service.startInFlight = false;
    if (startedRevision !== service.currentRevision) {
      // The session changed while the platform was creating the task: withdraw
      // it rather than adopt it into the new one.
      recordTask(KEEP_ALIVE_STAGE_START, KEEP_ALIVE_RESULT_SKIPPED, 'stale', service.cancelCount);
      await releaseTask(context);
      return false;
    }
    recordTask(KEEP_ALIVE_STAGE_START, KEEP_ALIVE_RESULT_OK, reason, service.cancelCount);
    hilog.info(DOMAIN, TAG, 'UI_BG_TASK_STARTED|id=%{public}d|notification=%{public}d',
      service.continuousTaskId, service.notificationId);
    // The task exists from here on: a failed live-view update is its own record
    // and never turns this start into a failure.
    await publishLiveView(context, keepAliveNotificationText(status), reason);
    return true;
  } catch (error) {
    service.startInFlight = false;
    recordTask(KEEP_ALIVE_STAGE_START, KEEP_ALIVE_RESULT_ERROR, failureToken(error as Error),
      service.cancelCount);
    return false;
  }
}

export async function refreshKeepAlive(context: common.UIAbilityContext,
  status: KeepAliveStatus, revision: number, reason: string): Promise<boolean> {
  service.context = context;
  service.currentRevision = revision;
  if (!service.hasTask) {
    recordTask(KEEP_ALIVE_STAGE_REFRESH, KEEP_ALIVE_RESULT_SKIPPED, 'no-task', service.cancelCount);
    return false;
  }
  return await publishLiveView(context, keepAliveNotificationText(status), reason);
}

// 0.0.20: the same refresh with a body the caller supplies. Used inside the
// reconnect grace, where there is no fresh status document to render (the tunnel
// is down and the page is waiting for the automatic reconnect), so the live view
// reports what the app is DOING — KEEP_ALIVE_NOTIFICATION_RECONNECTING — instead
// of freezing the last traffic snapshot the platform would then cancel for not
// being updated.
export async function refreshKeepAliveText(context: common.UIAbilityContext,
  text: string, revision: number, reason: string): Promise<boolean> {
  service.context = context;
  service.currentRevision = revision;
  if (!service.hasTask) {
    recordTask(KEEP_ALIVE_STAGE_REFRESH, KEEP_ALIVE_RESULT_SKIPPED, 'no-task', service.cancelCount);
    return false;
  }
  return await publishLiveView(context, text, reason);
}

// Release the task. Silent when there is nothing to release (the 2 s tick asks
// again and again once the tunnel is down — that is a no-op, not an event).
async function releaseTask(context: common.UIAbilityContext): Promise<boolean> {
  if (!service.hasTask && !service.startInFlight) {
    return false;
  }
  // Our own stop must not be read back as a platform cancellation.
  service.stopping = true;
  try {
    await backgroundTaskManager.stopBackgroundRunning(context);
    service.hasTask = false;
    service.notificationId = -1;
    service.continuousTaskId = -1;
    return true;
  } catch (error) {
    // The task may already be gone (a system cancellation we have not been told
    // about yet); the platform's next event, or the claim path, will correct the
    // state either way.
    recordTask(KEEP_ALIVE_STAGE_STOP, KEEP_ALIVE_RESULT_ERROR, failureToken(error as Error),
      service.cancelCount);
    return false;
  } finally {
    service.startInFlight = false;
    service.stopping = false;
  }
}

export async function stopKeepAlive(context: common.UIAbilityContext, revision: number,
  reason: string): Promise<boolean> {
  service.context = context;
  service.currentRevision = revision;
  if (!service.hasTask && !service.startInFlight) {
    return false;
  }
  const stopped: boolean = await releaseTask(context);
  if (stopped) {
    recordTask(KEEP_ALIVE_STAGE_STOP, KEEP_ALIVE_RESULT_OK, reason, service.cancelCount);
    hilog.info(DOMAIN, TAG, 'UI_BG_TASK_STOPPED|reason=%{public}s', reason);
  }
  return stopped;
}
