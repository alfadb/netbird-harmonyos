// The extension publishes every 5 seconds. A snapshot older than three ticks
// does not prove that its process (or VPN) is still running.
export const STATUS_MAX_AGE_MS: number = 15000;

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
