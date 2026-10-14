// The extension publishes every 5 seconds (one tick). A snapshot older than
// TWO ticks no longer proves its process (or the VPN) is still running — the
// stale "已连接" of a dead extension must decay to 未连接 within bounded time
// (roam-1005-0014 B.3; two ticks, not three, since 2026-10-05).
export const STATUS_MAX_AGE_MS: number = 10000;

interface StatusSnapshot {
  updated_at_ms: number;
}

export function currentPublishedStatus<T extends StatusSnapshot>(status: T | null, nowMs: number): T | null {
  if (status === null || !Number.isFinite(status.updated_at_ms) || status.updated_at_ms <= 0 ||
    !Number.isFinite(nowMs) || nowMs < status.updated_at_ms ||
    nowMs - status.updated_at_ms > STATUS_MAX_AGE_MS) {
    return null;
  }
  return status;
}

// ---------------------------------------------------------------------------
// 0.0.34 peer-status lag (device report `peer-status-lag/REPORT.md` §5 F1/F3/F4)
//
// The two policies below are the WHOLE of the shell-side lag fix, kept pure and
// import-free so the host selfcheck exercises the shipped code byte-identically:
//
//   * `shouldPublishStatus` — the WRITER's debounce (F4). The 5 s watcher
//     heartbeat stays (it is what keeps the document inside the freshness gate
//     above), but a tick whose document is already young enough that the gate
//     cannot lapse before the NEXT tick republishes is not rewritten: the edge
//     publication that just happened IS the write. The change key is cheap and
//     covers every input that reaches the document from outside the core's own
//     edge set — the core status revision, the vpn-tun verdict and the applied
//     network map's serial — so a peer-list or interface change is never
//     swallowed by a counter-oriented debounce.
//
//   * `statusDocumentStamp` / `shouldReapplyPublishedStatus` — the READER's
//     change skip (F3). The peer page now ticks at 1 s (Index.ets
//     `STATUS_POLL_MS`), which is what makes an edge publication visible within
//     ~1 s, but it only re-derives and re-assigns its UI state when the
//     document really moved (`updated_at_ms` + the core's `status_revision`) —
//     an idle screen therefore does LESS work than the old 2 s tick, not more.
// ---------------------------------------------------------------------------

/// The extra margin kept between the age a skipped heartbeat leaves behind and
/// the reader's gate ([`STATUS_MAX_AGE_MS`]): a late tick must not be able to
/// push the document past the gate, which would flip the page to 未连接.
export const STATUS_PUBLISH_FRESHNESS_SLACK_MS: number = 2000;

/// The age at which a heartbeat write becomes MANDATORY because the document
/// would otherwise lapse: the reader's gate minus one full watcher cadence
/// (the next publication opportunity) minus the jitter margin above. Negative
/// (a cadence longer than the gate) clamps to 0 = write on every tick.
export function statusHeartbeatMaxAgeMs(watcherCadenceMs: number): number {
  const max: number = STATUS_MAX_AGE_MS - watcherCadenceMs - STATUS_PUBLISH_FRESHNESS_SLACK_MS;
  return max > 0 ? max : 0;
}

/// The publish debounce's change key: every cheap input that can change what
/// the UI would render. `-1` stands for "the core/reader did not deliver that
/// fact" (an older core without `status_revision`, a map that is not available)
/// — it is a value like any other, so "absent" is stable and does not by itself
/// force a write.
export function statusPublishChangeKey(statusRevision: number | undefined,
  ifacePresence: string, networkSerial: number | undefined): string {
  const rev: number = typeof statusRevision === 'number' && Number.isFinite(statusRevision)
    ? statusRevision : -1;
  const serial: number = typeof networkSerial === 'number' && Number.isFinite(networkSerial)
    ? networkSerial : -1;
  return `${rev}|${ifacePresence}|${serial}`;
}

export interface StatusPublishInput {
  /// The change key of the document about to be written.
  changeKey: string;
  /// The change key of the last SUCCESSFUL write ('' = none in this process).
  lastChangeKey: string;
  nowMs: number;
  /// Date.now() of the last successful write (0 = none).
  lastPublishMs: number;
  /// From [`statusHeartbeatMaxAgeMs`].
  heartbeatMaxAgeMs: number;
}

/// Should this tick/edge actually WRITE the document? (Force-publish callers —
/// the engine's own start report — bypass this and must do so explicitly.)
export function shouldPublishStatus(input: StatusPublishInput): boolean {
  if (!Number.isFinite(input.nowMs) || input.nowMs <= 0) {
    return false;
  }
  // Nothing is on disk yet for the UI to reuse.
  if (input.lastPublishMs <= 0 || input.lastChangeKey.length === 0) {
    return true;
  }
  // A rendered input moved: this is the edge/tick that must be visible.
  if (input.changeKey !== input.lastChangeKey) {
    return true;
  }
  // Nothing moved. The heartbeat still owns the freshness gate: write once the
  // document is old enough that the NEXT watcher tick could otherwise let it
  // lapse.
  return input.nowMs - input.lastPublishMs >= input.heartbeatMaxAgeMs;
}

interface StatusStampSource {
  updated_at_ms: number;
  status_revision?: number;
}

/// The reader's document identity: the write stamp plus the core's edge
/// revision. `updated_at_ms` alone would collapse two publications landing in
/// the same millisecond; the revision alone would miss the interface/map
/// changes the revision does not cover. `''` = not a readable document.
///
/// GENERIC on purpose, exactly like [`currentPublishedStatus`] above: ArkTS
/// forbids structural typing (`arkts-no-structural-typing`), so the page's
/// `PublishedStatus` must be passed through an inferred type parameter rather
/// than converted to this interface.
export function statusDocumentStamp<T extends StatusStampSource>(
  doc: T | null | undefined): string {
  if (doc === null || doc === undefined || !Number.isFinite(doc.updated_at_ms) ||
    doc.updated_at_ms <= 0) {
    return '';
  }
  const rev: number = typeof doc.status_revision === 'number' &&
    Number.isFinite(doc.status_revision) ? doc.status_revision : -1;
  return `${doc.updated_at_ms}|${rev}`;
}

/// Should the reader re-derive and re-assign its UI state? Only when the live
/// document differs from the one already applied (see [`statusDocumentStamp`]).
export function shouldReapplyPublishedStatus(appliedStamp: string,
  nextStamp: string): boolean {
  return nextStamp.length > 0 && nextStamp !== appliedStamp;
}
