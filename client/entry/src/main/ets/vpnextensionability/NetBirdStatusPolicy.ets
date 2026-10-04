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
