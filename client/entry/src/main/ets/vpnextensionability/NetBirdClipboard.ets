/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// 0.0.43 (I8) §3 — the ONE place this app writes to the system pasteboard.
//
// WHY A MODULE AND NOT AN INLINE CALL. The copy affordance is a UI rule (which
// row shows an icon, when it is visible) but the WRITE is a platform seam, and
// this project's rule is that a seam is one import site that the host selfcheck
// can stage and stub — the same shape as NetBirdKeepAliveStore's file IO. So the
// page owns the policy and the toast, and this file owns exactly one call.
//
// THE API IS PUBLIC AND NEEDS NO ACL. Verified against the local SDK
// (/home/worker/harmonyos/command-line-tools/current/sdk/default/openharmony/ets/api):
//
//   @ohos.pasteboard.d.ts:71    const MIMETYPE_TEXT_PLAIN = 'text/plain'
//                               (@since 7, @atomicservice since 11, no @permission)
//   @ohos.pasteboard.d.ts:163   function createData(mimeType: string,
//                                 value: ValueType): PasteData
//                               (@since 9, @atomicservice since 11, no @permission)
//   @ohos.pasteboard.d.ts:254   function getSystemPasteboard(): SystemPasteboard
//                               (@since 6, @atomicservice since 11, no @permission)
//   @ohos.pasteboard.d.ts:1434  setDataSync(data: PasteData): void
//                               (@since 11, @atomicservice, no @permission)
//
// The PERMISSIONED half of the pasteboard is the READING side: the getData /
// getDataSync family carries `@permission ohos.permission.READ_PASTEBOARD`
// (@ohos.pasteboard.d.ts:1438 onwards). Nothing here reads the clipboard, so this
// module needs no permission entry in module.json5 and no ACL — it is usable by a
// normal app exactly as the SDK documents it. The kit form of the same module
// (@kit.BasicServicesKit.d.ts:28 imports and :60 re-exports `pasteboard`) is
// equivalent; the @ohos.* specifier is used because every other adapter in this
// project imports its seam that way.
//
// WHY setDataSync: it is the public synchronous writer (@since 11), so the call
// path this file takes is the one the host selfcheck can observe end-to-end
// through a recording stub. The promise form (setData, :1421) is the same write
// with an await the caller has no use for — the page's confirmation toast is
// transient either way, and a refusal (12900005 excessive time, 27787278
// replication prohibited) is caught by the caller and never surfaced as an error
// dialog, exactly like the page's existing toasts.

import pasteboard from '@ohos.pasteboard';

/** The MIME type every copied value in this app uses — plain text, nothing else. */
export const CLIPBOARD_TEXT_MIME: string = pasteboard.MIMETYPE_TEXT_PLAIN;

/**
 * Writes EXACTLY `text` to the system pasteboard and reports whether a write was
 * attempted. An empty string is refused here rather than written: putting "" on
 * the clipboard would silently replace whatever the user had.
 *
 * Errors are NOT swallowed here — the caller's try/catch owns the fail-soft
 * policy (a refused write must never break the row it was invoked from).
 */
export function writeClipboardText(text: string): boolean {
  if (text.length === 0) {
    return false;
  }
  const data: pasteboard.PasteData = pasteboard.createData(CLIPBOARD_TEXT_MIME, text);
  pasteboard.getSystemPasteboard().setDataSync(data);
  return true;
}
