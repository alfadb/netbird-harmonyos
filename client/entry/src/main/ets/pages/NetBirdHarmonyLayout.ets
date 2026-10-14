// NetBirdHarmonyLayout — the responsive shell's DECISIONS, as pure functions.
//
// 0.0.36 (I1 of DESIGN-HARMONY): one page that is a phone layout, a tablet layout
// and a 2in1 two-pane shell, decided by the WINDOW's width in vp. Everything in
// this file is numbers in, decisions out: no ArkUI component, no page state, no
// `$r()` resource — so the host selfcheck
// (client/entry/selfcheck/run-harmony-layout.mts) imports this file
// byte-identical and pins every boundary. Same discipline as
// NetBirdStatusPolicy.ets / NetBirdKeepAlivePolicy.ets.
//
// 0.0.37 (I2): the rail width follows the window in THREE tiers instead of one
// constant, the pane's default destination is a function of the CONNECTION STATE,
// and the visual scale (type sizes, spacing, the label/value row that must not
// break a word) lives here as named numbers instead of ad-hoc literals.
//
// 0.0.38 (I3): the content DECISIONS that finish the wide layout — the peer list
// column's width (which is what makes a real peer name fit), the two-line row's
// geometry and its "the IP never wraps" floor, the 连接概览 pane's ROW POLICY
// (which rows exist for a given document, so nothing is invented), the peer
// detail's default/advanced split as a classifier over a row's label, the
// settings column's width clamp and the rail's connection-region cap.
//
// 0.0.39 (I4): the independent visual review (pc-ui-design/K3-REVIEW-I2.md) read
// the I2 build and its P0.2 asked for TWO things the I3 column could not do at
// once — the column becomes a TIERED table (a cap per rail tier, plus its own
// per-tier reservation of the detail pane's minimum), and the ROW itself is
// condensed at the narrowest wide tier so the name column reaches a floor that is
// stated as "what a 28-character fqdn needs at 14 vp". The same review's P0.3/P1
// set the settings column to 680, the 关于 values to the row's end, the rail's
// text to ONE axis, the peer detail to four named sections and the connect disc's
// fill/ring split. Every one of those numbers lives here, so the host selfcheck
// can assert them instead of a screenshot.
//
// Nothing here reads `deviceInfo.deviceType`: the layout follows the WINDOW, never
// the device (a 2in1 window can be dragged through every tier — DESIGN-HARMONY
// §A.2/§C.2). deviceType stays where it is already correct: the tray gate and
// keepAliveModeFor.
//
// THE BREAKPOINTS ARE THE OFFICIAL ONES. Names and ranges are the
// `WidthBreakpoint` enum's own documentation (component/enums.d.ts:4741-4792,
// SDK API 26, @since 13):
//
//   xs  (0, 320)      WIDTH_XS = 0   →  narrow branch (today's bottom-tab shell)
//   sm  [320, 600)    WIDTH_SM = 1   →  narrow branch
//   md  [600, 840)    WIDTH_MD = 2   →  narrow branch (see below)
//   lg  [840, 1440)   WIDTH_LG = 3   →  split: rail + pane
//   xl  [1440, +inf)  WIDTH_XL = 4   →  split: rail + pane
//
// `md` deliberately keeps the PHONE shell: the official 底部与侧边导航 scenario
// says bottom navigation is right for "sm 或 md" and the left rail starts at lg,
// and 600 vp is also NavigationMode.Auto's own split threshold.
//
// THE RAIL TIER TABLE (0.0.37) — window wider ⇒ roomier rail
// ----------------------------------------------------------
//   window width vp     rail vp   pane = window − rail   pane ≥ 600 (Auto split)?
//   [840, 1024)          240      600 … 783              yes, 600 exactly at 840
//   [1024, 1440)         280      744 … 1159             yes, 744 exactly at 1024
//   [1440, +inf)         320      ≥ 1120                 yes
//
// The three rail widths are a monotone step function of the window, and the pane
// never drops below NavigationMode.Auto's hard `>= 600 vp` rule — including AT
// each tier boundary, where the rail jumps by 40 vp and the pane drops by the
// same 40 (1023→1024: 783→744; 1439→1440: 1159→1120). The binding case is the
// very bottom of the split shell:
//
//   RAIL_WIDTH_COMPACT_VP (240) + NAV_AUTO_SPLIT_VP (600) = 840 = HARMONY_BP_LG_VP
//
// i.e. at 840 vp the pane is EXACTLY 600 vp = minNavBarWidth (240) + minContentWidth
// (360), which is exactly the framework's documented Auto-split threshold
// ("If the window width is greater than or equal to 600 vp, the Split mode is
// used ... 600 vp = minNavBarWidth (240 vp) + minContentWidth (360 vp)",
// navigation.d.ts NavigationMode.Auto). The arithmetic is exact, not a near miss:
// 240 and 840 are integers, so the framework's own `>= 600` comparison cannot
// lose to a rounding error, and no part of `lg` runs the stack-in-pane fallback
// that would blank the list on a row tap (the k3 1.3 correction). That is why the
// compact tier is 240 and not k3's suggested 300/280/320 (which would need 900 vp
// and leave the whole bottom fifth of lg in Stack mode).
//
// The 1024 boundary is NOT an SDK breakpoint: it is the point where a 240 vp rail
// stops looking cramped against a 1024 vp window (the small-laptop / half-screen
// 2in1 size), and 1440 is the official `lg → xl` boundary — from there the window
// is a desktop window and the rail gets the room the design's original 320 vp
// wanted. The rail grows only in the direction the window does, so dragging a
// window wider never shrinks the rail.
//
// THE xl INDENT LIMIT (缩进布局, DESIGN-HARMONY §C.1)
// --------------------------------------------------
// From xl (≥1440 vp) the content pane is indent-limited: it stops growing at
// CONTENT_MAX_WIDTH_VP (1200) and is centred in the remaining width, so a 2560 vp
// desktop window gets readable line lengths instead of a 2000 vp-wide card. The
// limit only bites once the pane is wider than 1200 vp (window > rail + 1200).

// ---- the official horizontal breakpoint boundaries (vp) ---------------------
/** xs → sm: the phone-size boundary of `WidthBreakpoint` (0, 320). */
export const HARMONY_BP_SM_VP: number = 320;
/** sm → md: documented as `WidthBreakpoint` sm's upper bound (320, 600). */
export const HARMONY_BP_MD_VP: number = 600;
/** md → lg: `WidthBreakpoint` md's upper bound (600, 840); the split threshold. */
export const HARMONY_BP_LG_VP: number = 840;
/** lg → xl: `WidthBreakpoint` lg's upper bound (840, 1440). */
export const HARMONY_BP_XL_VP: number = 1440;

export const BREAKPOINT_XS: string = 'xs';
export const BREAKPOINT_SM: string = 'sm';
export const BREAKPOINT_MD: string = 'md';
export const BREAKPOINT_LG: string = 'lg';
export const BREAKPOINT_XL: string = 'xl';

// ---- the two arrangements ---------------------------------------------------
/** Today's shell: the bottom-tab `Tabs` (xs/sm/md, and a narrow 2in1 window). */
export const PANE_SINGLE: string = 'single';
/** The new shell: `Row { connectionRail(); contentPane() }` (lg/xl). */
export const PANE_SPLIT: string = 'split';

// ---- the rail's three tiers (0.0.37) ----------------------------------------
export const RAIL_TIER_COMPACT: string = 'compact';
export const RAIL_TIER_MEDIUM: string = 'medium';
export const RAIL_TIER_ROOMY: string = 'roomy';
/** compact → medium: the window width where a 240 vp rail starts to look cramped. */
export const RAIL_TIER_MID_VP: number = 1024;
/** [840, 1024): the official `minNavBarWidth` default — what makes 240+600=840 work. */
export const RAIL_WIDTH_COMPACT_VP: number = 240;
/** [1024, 1440): a 40 vp step up; the pane keeps ≥ 744 vp, well above the Auto rule. */
export const RAIL_WIDTH_MEDIUM_VP: number = 280;
/** [1440, +inf): the roomy desktop rail; the pane keeps ≥ 1120 vp. */
export const RAIL_WIDTH_ROOMY_VP: number = 320;

// ---- the split branch's width budget (all vp) -------------------------------
/** The 对端 pane's navBar (today's peer list). 240 vp = Navigation's own default. */
export const NAV_BAR_WIDTH_VP: number = 240;
/** The pane Navigation's minContentWidth (today's peer detail). 360 vp = default. */
export const MIN_CONTENT_WIDTH_VP: number = 360;
/**
 * NavigationMode.Auto's split threshold: a hard `>= 600 vp` in API 10+, documented
 * as minNavBarWidth (240) + minContentWidth (360) — it does NOT move when
 * navBarWidth/minContentWidth are set (only AUTO_WITH_ASPECT_RATIO derives it).
 */
export const NAV_AUTO_SPLIT_VP: number = 600;
/** The official `lg` start — where the shell switches to the two panes. */
export const SPLIT_MIN_WINDOW_VP: number = HARMONY_BP_LG_VP;
/** The xl indent limit (缩进布局): the pane stops growing here and centres. */
export const CONTENT_MAX_WIDTH_VP: number = 1200;

/**
 * The official breakpoint NAME for a window width, `'xs'` … `'xl'`.
 * A non-finite or negative width classifies as `xs` (the narrow branch): a
 * mis-classified first frame shows the phone shell, never a half-built split.
 */
export function widthBreakpointOf(widthVp: number): string {
  if (!(widthVp >= HARMONY_BP_SM_VP)) {
    return BREAKPOINT_XS;
  }
  if (widthVp < HARMONY_BP_MD_VP) {
    return BREAKPOINT_SM;
  }
  if (widthVp < HARMONY_BP_LG_VP) {
    return BREAKPOINT_MD;
  }
  if (widthVp < HARMONY_BP_XL_VP) {
    return BREAKPOINT_LG;
  }
  return BREAKPOINT_XL;
}

/**
 * The effective window width (vp) at which the split shell is used: the larger of
 * the official `lg` boundary and the width the COMPACT rail plus the pane's
 * Navigation really need (RAIL_WIDTH_COMPACT_VP + NAV_AUTO_SPLIT_VP). Both are
 * 840, which is the point: the shell's threshold and the pane's own split
 * threshold coincide, so the bottom of `lg` is inside the split rather than
 * outside it — at every rail tier, because the rail only grows where the pane has
 * room to spare (see the tier table above).
 */
export function splitThresholdVp(): number {
  const needed: number = RAIL_WIDTH_COMPACT_VP + NAV_AUTO_SPLIT_VP;
  return needed > SPLIT_MIN_WINDOW_VP ? needed : SPLIT_MIN_WINDOW_VP;
}

/**
 * `'split'` (rail + pane) or `'single'` (today's bottom-tab shell) for a window
 * width in vp.
 */
export function paneLayoutOf(widthVp: number): string {
  return widthVp >= splitThresholdVp() ? PANE_SPLIT : PANE_SINGLE;
}

/**
 * The rail's tier name for a window width: `'compact'` (< 1024), `'medium'`
 * (< 1440) or `'roomy'` (≥ 1440). Below the split threshold the name is the
 * compact tier (the rail is not mounted, but the name stays total).
 */
export function railTierOf(widthVp: number): string {
  if (!(widthVp >= RAIL_TIER_MID_VP)) {
    return RAIL_TIER_COMPACT;
  }
  if (widthVp < HARMONY_BP_XL_VP) {
    return RAIL_TIER_MEDIUM;
  }
  return RAIL_TIER_ROOMY;
}

/**
 * The rail width (vp) for a window width — 240 / 280 / 320 by tier. Monotone in
 * the window width: a wider window never gets a narrower rail.
 */
export function railWidthFor(widthVp: number): number {
  const tier: string = railTierOf(widthVp);
  if (tier === RAIL_TIER_ROOMY) {
    return RAIL_WIDTH_ROOMY_VP;
  }
  if (tier === RAIL_TIER_MEDIUM) {
    return RAIL_WIDTH_MEDIUM_VP;
  }
  return RAIL_WIDTH_COMPACT_VP;
}

/**
 * The width (vp) left for the content pane once the rail is mounted: the window
 * minus its tier's rail. Only meaningful for a window that is actually in the
 * split shell — `paneSplitValid` is the guard.
 */
export function splitPaneWidthVp(widthVp: number): number {
  return widthVp - railWidthFor(widthVp);
}

/**
 * The invariant the tier table exists to protect: wherever the split shell is
 * mounted (width ≥ 840), the pane is at least `NAV_AUTO_SPLIT_VP` (600) wide, so
 * `NavigationMode.Auto` really renders the 对端 list beside the detail instead of
 * falling back to Stack mode and blanking the list on a row tap. Below the split
 * threshold the shell is not mounted and the property holds vacuously.
 */
export function paneSplitValid(widthVp: number): boolean {
  if (paneLayoutOf(widthVp) !== PANE_SPLIT) {
    return true;
  }
  return splitPaneWidthVp(widthVp) >= NAV_AUTO_SPLIT_VP;
}

/**
 * The content pane's max width (vp): 0 = unconstrained (below xl), else the xl
 * indent limit. The page renders `{}`/no constraint for 0 rather than a
 * `maxWidth: 0` that would collapse the pane.
 */
export function contentMaxWidthVp(widthVp: number): number {
  return widthVp >= HARMONY_BP_XL_VP ? CONTENT_MAX_WIDTH_VP : 0;
}

// ---------------------------------------------------------------------------
// The state-driven default destination (0.0.37, I2)
// ---------------------------------------------------------------------------
// The pane is NOT always the peer list. With no explicit user choice the pane
// shows what the current connection state makes useful:
//
//   connection state                         default pane
//   connected (core connected + own IP        对端 (the peer list)
//     + vpn-tun interface present)
//   everything else (未连接 / 连接中… /       the GUIDE for that state
//     正在断开… / 需要登录 / 尚未配置)          (a real block: the state, the
//                                              last failure reason, how to
//                                              connect, the login entry)
//
// An explicit choice (对端 or 设置) wins FOR THE SESSION: `activeTab` 1 or 2 is
// returned unchanged whatever the connection state does. `activeTab` 0 means
// "the user has not chosen a destination" — 主页 is the rail itself — and on a
// fresh app start that is exactly the initial value, so the default applies.
// The resolution is a pure function of (activeTab, connected, needsLogin) and is
// re-evaluated on every publish tick, so a connection that comes up flips the
// untouched default from the guide to the peer list, and a tunnel that drops
// flips it back.
//
// The destination numbers ARE `activeTab`'s numbers (0/1/2): 0 = the guide the
// rail itself stands for, 1 = 对端, 2 = 设置.
export const PANE_DEST_GUIDE: number = 0;
export const PANE_DEST_PEERS: number = 1;
export const PANE_DEST_SETTINGS: number = 2;

/** Which guide the default pane shows when no destination is chosen. */
export const GUIDE_LOGIN: string = 'login';
export const GUIDE_CONNECT: string = 'connect';

export interface WidePaneFacts {
  /** The page's own `activeTab` (0 主页 / 1 对端 / 2 设置). */
  activeTab: number;
  /** The FULLY connected gate: core state connected + own overlay IP + iface present. */
  connected: boolean;
  /** The core's `needs-login` class (the official Login required verdict). */
  needsLogin: boolean;
}

export function widePaneDestination(facts: WidePaneFacts): number {
  if (facts.activeTab === PANE_DEST_PEERS || facts.activeTab === PANE_DEST_SETTINGS) {
    return facts.activeTab;
  }
  return facts.connected ? PANE_DEST_PEERS : PANE_DEST_GUIDE;
}

/**
 * The guide variant for the default pane: the login guide when the core reported
 * `needs-login` (the account entry belongs there), else the honest connect guide
 * whose headline is the current state text.
 */
export function wideGuideKind(facts: WidePaneFacts): string {
  return facts.needsLogin ? GUIDE_LOGIN : GUIDE_CONNECT;
}

// ---------------------------------------------------------------------------
// The visual scale (0.0.37) — HarmonyOS conventions, no ad-hoc sizes
// ---------------------------------------------------------------------------
// UX standard 2.1.4.2: 手机/折叠/平板 ≥ 12 vp (推荐) and 电脑 ≥ 14 vp (推荐) for
// text; the review (K3-REVIEW-HARMONY §C.5) flagged today's 12–13 vp muted text
// as below the PC recommendation. One scale, used by the page, keeps the PC floor
// and the phone floor satisfied by the same numbers instead of drifting per call
// site.
/** The app title inside the rail ("NetBird"). */
export const TYPE_APP_TITLE_VP: number = 22;
/** A page/section headline (对端, 设置, 对端详情, the guide's state). */
export const TYPE_PAGE_TITLE_VP: number = 20;
/** A settings group header above its cards. */
export const TYPE_SECTION_TITLE_VP: number = 16;
/** Body text and the primary value of a label/value row. */
export const TYPE_BODY_VP: number = 15;
/** Secondary/caption text: ≥ 12 vp everywhere, ≥ 14 vp on the PC (this value). */
export const TYPE_CAPTION_VP: number = 14;
/** The 连接概览 pane's stat-tile number (the review's 24 vp Medium tile figure). */
export const TYPE_STAT_VALUE_VP: number = 24;

// ---- the rail's inner width and the label/value policy ----------------------
/**
 * The rail's page padding (8 vp grid; the I1 rail used 20, which is off-grid and
 * ate the value column of the info rows).
 */
export const RAIL_PAGE_PADDING_VP: number = 16;
/** The rail cards' inner padding (8 vp grid). */
export const RAIL_CARD_PADDING_VP: number = 16;
/** The floor the label/value rows need to render a value on ONE line. */
export const RAIL_INFO_MIN_VALUE_VP: number = 160;

// ---- the rail's ONE text axis (0.0.39, I4) ---------------------------------
// The review's §5.1: the rail's text started on three different x positions —
// the card's text at 32 (page 16 + card 16), the 导航 caption at 24, the nav
// labels at 35 — and the wordmark was centred. One axis at 32, expressed here so
// the page can be laid out from it and the selfcheck can assert the sum:
//
//   caption      RAIL_DESTINATIONS_PADDING_VP + RAIL_CARD_PADDING_VP     = 32
//   nav label    RAIL_DESTINATIONS_PADDING_VP + RAIL_NAV_ROW_PADDING_VP
//                  + RAIL_NAV_BAR_WIDTH_VP + RAIL_NAV_LABEL_GAP_VP        = 32
//   wordmark     RAIL_PAGE_PADDING_VP + RAIL_CARD_PADDING_VP             = 32
//   card text    RAIL_PAGE_PADDING_VP + RAIL_CARD_PADDING_VP             = 32
/** The rail's one text axis (vp): every rail text line starts here. */
export const RAIL_TEXT_AXIS_VP: number = 32;
/** The destinations block's outer padding (vp) — the axis minus the card padding. */
export const RAIL_DESTINATIONS_PADDING_VP: number =
  RAIL_TEXT_AXIS_VP - RAIL_CARD_PADDING_VP;
/** The nav row's own left padding (vp): 0, so the accent bar hugs the block edge. */
export const RAIL_NAV_ROW_PADDING_VP: number = 0;
/** The selected accent bar's width (vp). */
export const RAIL_NAV_BAR_WIDTH_VP: number = 3;
/** The gap between that bar and the nav label (vp) — what lands the label on 32. */
export const RAIL_NAV_LABEL_GAP_VP: number =
  RAIL_TEXT_AXIS_VP - RAIL_DESTINATIONS_PADDING_VP - RAIL_NAV_BAR_WIDTH_VP;
/** The rail/content seam's stroke (vp): the rail and the pane share one surface. */
export const RAIL_SEAM_WIDTH_VP: number = 1;

// ---- the connect control's state ring (0.0.39, I4) -------------------------
// The review's §4.3: the 104 vp disc painted the CURRENT STATE as its fill while
// its label is the INVERSE ACTION (断开 on green), so it read as a lamp and it
// outranked the status card it exists to support. Fill = the primary action role
// always; the state moves to this ring in the state colour.
/** The connect disc's state ring (vp). */
export const CONNECT_RING_WIDTH_VP: number = 2;

/**
 * The width (vp) an info row's VALUE gets in the rail's card: the rail width
 * minus the page padding and the card padding on both sides. The I1 device report
 * was this number being 40–72 vp too small: "对端 0/0 已连接" broke as 已连/接
 * because the label column (96 vp) was carved out of the row BEFORE the value
 * (DESIGN-HARMONY I1 screenshot). The fix is a STACKED row — the 14 vp label as a
 * caption ABOVE the value — so the value owns this full width; the selfcheck
 * asserts the floor at every rail tier, which is the testable form of "the row
 * cannot break a word" (same role as splitThresholdVp for the width math).
 */
export function railInfoValueWidthVp(widthVp: number): number {
  return railWidthFor(widthVp) - 2 * RAIL_PAGE_PADDING_VP - 2 * RAIL_CARD_PADDING_VP;
}

// ---------------------------------------------------------------------------
// 0.0.38 (I3) — the peer LIST COLUMN and its TWO-LINE row
// ---------------------------------------------------------------------------
// The I2 device screenshot carried two defects in the peer list, and both of
// them are the list column being too narrow for what I2 put inside it:
//
//   * the NAME rendered as "blkint…" — six characters of a 25-character fqdn.
//     The name's column was what was left of the framework's 240 vp navBar after
//     the page padding (32), the list card padding (32), the 直连/中继 chip, the
//     status text and the status dot had taken their share;
//   * the IP BROKE ACROSS LINES ("100.108." / "186.66") — it shared the name's
//     squeezed column instead of owning a line of its own.
//
// The layout answer is both halves at once, which is why they live here:
//
//   1. the row is TWO-LINE and `PEER_ROW_MIN_HEIGHT_VP` is the compact 64 vp; the
//      IP owns line 2 and therefore the row's FULL text width, so it cannot wrap;
//   2. the list column is widened from Navigation's 240 vp default to as much of
//      the pane as it can have WITHOUT dropping the detail pane below
//      MIN_CONTENT_WIDTH_VP (360) — the one number NavigationMode.Auto's hard
//      ">= 600 vp" split is calibrated on (240 + 360). The column therefore never
//      breaks the split, and it follows the pane up to the cap that keeps the
//      detail pane the larger half.
//
// The two questions the page asks and the selfcheck pins are "how wide is the
// column" (peerListNavBarWidthFor) and "does what the row renders fit in it"
// (peerRowNameWidthVp / peerRowTextWidthVp against their floors) — so the "IP
// never wraps" and "real names fit" requirements become arithmetic instead of a
// screenshot someone has to squint at.

// 0.0.39 (I4) — THE TIERED LIST COLUMN AND THE 28-CHARACTER NAME FLOOR
// ---------------------------------------------------------------------------
// I3 widened the column to `pane - 360` (capped 420), which is right for a
// 1024 vp+ window and does NOTHING at the bottom of the shell: at 840 vp the rail
// is 240, the pane is exactly 600, and the detail pane's own 360 vp reservation
// leaves the list column at 240 — the same column the review measured, with the
// name down to ~64 vp (the "blkint…" of §P0.1). Three things fix it, and all
// three are arithmetic here:
//
//   1. THE COLUMN IS A TIER TABLE (like the rail's own 240/280/320): a cap per
//      rail tier — 360 / 440 / 480 — monotone in the window, so the list is never
//      narrower than I3's at any window and a 4000 vp desktop window cannot give
//      the list a 2000 vp column.
//   2. THE COMPACT TIER RESERVES LESS FOR THE DETAIL. NavigationMode.Auto derives
//      its split from the DEFAULTS (240 + 360 = 600) and clamps the real
//      navBarWidth to `pane - minContentWidth - 1`, so the reservation is ours to
//      set per tier: at the compact tier the pane's 600 vp is split 280 (list) /
//      320 (detail) instead of 240 / 360. The 40 vp the list gains is exactly what
//      makes a real fqdn fit; the detail keeps 320 vp, the width the review's own
//      numbers leave it, and the pane still splits (320 > 0 and the framework's
//      `>= 600 vp` rule is measured on the PANE, which never moves).
//   3. THE ROW IS CONDENSED AT THAT TIER. The 直连/中继 chip and the 空闲/离线 text
//      move from line 1 to line 2 (beside the IP, which only needs its own 120 vp
//      floor), leaving line 1's name all but the status dot. Line 2 still holds
//      everything it did (the IP never wraps: text >= 120 + 88 at every tier).
//
// The floor itself is stated the way the review stated it — "what a 28-character
// fqdn needs at 14 vp" — as 28 x 7 vp (0.5 em, the proportional-Latin rule of
// thumb, at the 14 vp caption size the PC floor is set in). The selfcheck sweeps
// EVERY window of the shell (840 … 4000) and asserts the name column clears it,
// so "a real name fits" is a property of the table, not of one screenshot.

/** The peer list column's cap (vp) at the compact rail tier — [840, 1024). */
export const PEER_LIST_NAV_BAR_CAP_COMPACT_VP: number = 360;
/** The peer list column's cap (vp) at the medium rail tier — [1024, 1440). */
export const PEER_LIST_NAV_BAR_CAP_MEDIUM_VP: number = 440;
/** The peer list column's cap (vp) at the roomy rail tier — [1440, +inf). */
export const PEER_LIST_NAV_BAR_CAP_ROOMY_VP: number = 480;
/**
 * The compact tier's reservation for the detail pane (vp). 360 is the framework's
 * own minContentWidth default and stays the reservation at the two wider tiers;
 * the compact tier reserves 320 so the 600 vp pane at the bottom of the shell
 * splits 280 / 320 — the same 600 the Auto rule requires, redistributed towards
 * the list (the review's §P0.1: "the navBar is the bottleneck, not the rail").
 */
export const PEER_LIST_CONTENT_MIN_COMPACT_VP: number = 320;
/** The peer list card's own left/right padding (vp) — 8 vp grid. */
export const PEER_ROW_LIST_PADDING_VP: number = 12;
/** The two-line peer row's minimum height (vp): name+chip+status / IP. */
export const PEER_ROW_MIN_HEIGHT_VP: number = 64;
/**
 * Line 1's right-hand block at the WIDE tiers (medium/roomy): the 直连/中继 chip,
 * the 空闲/离线 status text, the status dot and their gaps. The 已连接 TEXT no
 * longer renders at all (the dot and the chip already say it twice — the review's
 * §3.1), but the budget keeps room for the longest of the other two states, so a
 * row that shows 离线 cannot push the name below its floor.
 */
export const PEER_ROW_META_WIDE_VP: number = 112;
/**
 * Line 1's right-hand block at the COMPACT tier: the status dot and its gap only.
 * The chip and any 空闲/离线 text ride line 2 (see PEER_ROW_L2_META_COMPACT_VP).
 */
export const PEER_ROW_META_COMPACT_VP: number = 18;
/**
 * Line 2's right-hand block at the COMPACT tier: the 直连/中继 chip, the
 * 空闲/离线 text and their gaps, beside the IP. Line 2's text width minus this is
 * still >= the IP's own one-line floor at every window of the shell.
 */
export const PEER_ROW_L2_META_COMPACT_VP: number = 88;
/** The floor line 2 needs for a dotted-quad IPv4 on ONE line at caption size. */
export const PEER_ROW_IP_MIN_WIDTH_VP: number = 120;
/** The characters a real peer fqdn is measured in (the review's 28-char name). */
export const PEER_ROW_FQDN_CHARS: number = 28;
/**
 * The width budget of one fqdn character (vp) at the 14 vp the floor is stated
 * in: 0.5 em, the proportional-Latin rule of thumb. A 28-character self-hosted
 * fqdn (dots and hyphens are NARROWER than this, so the budget over-estimates).
 */
export const PEER_ROW_NAME_CHAR_VP: number = 7;
/** The floor line 1's name column must clear: a real 28-character fqdn at 14 vp. */
export const PEER_ROW_NAME_MIN_WIDTH_VP: number =
  PEER_ROW_FQDN_CHARS * PEER_ROW_NAME_CHAR_VP;
/** The search field's own cap (vp): a full-width box in a narrow list is noise. */
export const PEER_SEARCH_MAX_WIDTH_VP: number = 320;

/** The peer list column's cap (vp) for a window width — one per rail tier. */
export function peerListNavBarCapVp(windowVp: number): number {
  const tier: string = railTierOf(windowVp);
  if (tier === RAIL_TIER_ROOMY) {
    return PEER_LIST_NAV_BAR_CAP_ROOMY_VP;
  }
  if (tier === RAIL_TIER_MEDIUM) {
    return PEER_LIST_NAV_BAR_CAP_MEDIUM_VP;
  }
  return PEER_LIST_NAV_BAR_CAP_COMPACT_VP;
}

/**
 * The width (vp) the detail pane keeps at a window width: the compact tier's
 * 320, the framework's own 360 from the medium tier up. This is what the page
 * hands Navigation's `minContentWidth` — the value the framework derives the real
 * navBarWidth from (`calcNavBarWidth = width - minContentWidth - divider`), so it
 * is the number that decides the split, not a comment about it.
 */
export function peerContentMinWidthVp(windowVp: number): number {
  return railTierOf(windowVp) === RAIL_TIER_COMPACT
    ? PEER_LIST_CONTENT_MIN_COMPACT_VP : MIN_CONTENT_WIDTH_VP;
}

/**
 * The peer list column's width (vp) for a pane width at a window's tier:
 * everything the pane can give it above that tier's reservation, capped so the
 * list never eats the detail. Below the floor (a pane narrower than 240 + its
 * reservation, which the split shell never mounts) the answer is the framework
 * default, and a nonsense pane width answers the same — never NaN, never a
 * negative width.
 */
export function peerListNavBarWidthVp(paneWidthVp: number, windowVp: number): number {
  const room: number = paneWidthVp - peerContentMinWidthVp(windowVp);
  if (!(room >= NAV_BAR_WIDTH_VP)) {
    return NAV_BAR_WIDTH_VP;
  }
  const cap: number = peerListNavBarCapVp(windowVp);
  return room < cap ? room : cap;
}

/**
 * The peer list column's width (vp) for a WINDOW width — the pane's own width
 * first (`splitPaneWidthVp`), which is what makes the column a function of the
 * rail tier table and therefore of the window.
 */
export function peerListNavBarWidthFor(windowVp: number): number {
  return peerListNavBarWidthVp(splitPaneWidthVp(windowVp), windowVp);
}

/**
 * True when the peer row must use the CONDENSED form: the split shell's compact
 * rail tier. The narrow branch (the phone's bottom-tab list) is never condensed —
 * its rows own the whole page width and keep I3's shape.
 */
export function peerRowCondensed(windowVp: number): boolean {
  return paneLayoutOf(windowVp) === PANE_SPLIT &&
    railTierOf(windowVp) === RAIL_TIER_COMPACT;
}

/**
 * The width (vp) one peer row's TEXT has: the list column minus the page padding
 * and the list card padding on both sides. Line 2 (the NetBird IP) owns this full
 * width, so `>= PEER_ROW_IP_MIN_WIDTH_VP` is exactly "the IP never wraps".
 */
export function peerRowTextWidthVp(navBarWidthVp: number): number {
  return navBarWidthVp - 2 * RAIL_PAGE_PADDING_VP - 2 * PEER_ROW_LIST_PADDING_VP;
}

/** Line 1's right-hand block (vp) at a window's tier — see the two constants. */
export function peerRowMetaWidthVp(windowVp: number): number {
  return peerRowCondensed(windowVp) ? PEER_ROW_META_COMPACT_VP : PEER_ROW_META_WIDE_VP;
}

/**
 * The width (vp) line 1's NAME has at a window width: the row's text width at that
 * window's column, minus line 1's chip/dot block at that window's tier.
 * `>= PEER_ROW_NAME_MIN_WIDTH_VP` is exactly "a real 28-character fqdn fits", and
 * the selfcheck sweeps every window of the shell to hold it there.
 */
export function peerRowNameWidthVp(navBarWidthVp: number, windowVp: number): number {
  return peerRowTextWidthVp(navBarWidthVp) - peerRowMetaWidthVp(windowVp);
}

/** The name column's width (vp) for a window width — the composed form. */
export function peerRowNameWidthFor(windowVp: number): number {
  return peerRowNameWidthVp(peerListNavBarWidthFor(windowVp), windowVp);
}

/**
 * The search field's width (vp): the list column minus the page padding, capped
 * at `PEER_SEARCH_MAX_WIDTH_VP`. The page renders nothing at all when there is
 * no peer to search (see Index.peerListBody) — a search box over an empty list is
 * the one thing it can never be useful for.
 */
export function peerSearchWidthVp(navBarWidthVp: number): number {
  const full: number = navBarWidthVp - 2 * RAIL_PAGE_PADDING_VP;
  if (!(full > 0)) {
    return 0;
  }
  return full < PEER_SEARCH_MAX_WIDTH_VP ? full : PEER_SEARCH_MAX_WIDTH_VP;
}

// ---------------------------------------------------------------------------
// 0.0.38 (I3) — the SETTINGS content column
// ---------------------------------------------------------------------------
/**
 * The settings column's width cap (vp). I3 used 840; the review's §3.3 measured
 * the result and asked for 680 — a form column, not a page. The page CENTRES the
 * capped column inside the pane (the same 缩进布局 alignment the xl pane already
 * uses) rather than left-aligning it, because a left-aligned 680 column against a
 * 2560 vp pane reads as a half-drawn page.
 */
export const SETTINGS_CONTENT_MAX_WIDTH_VP: number = 680;

/**
 * The settings column's `maxWidth` for a pane width: 0 = NO constraint (the
 * column keeps its 100%), which is what a pane at or below the cap gets — a
 * `maxWidth` larger than the available width constrains nothing and only makes
 * the code look like it is doing something. The clamp starts to bite from
 * `pane > 680`.
 */
export function settingsContentMaxWidthVp(paneWidthVp: number): number {
  return paneWidthVp > SETTINGS_CONTENT_MAX_WIDTH_VP ? SETTINGS_CONTENT_MAX_WIDTH_VP : 0;
}

/**
 * A normal-width button inside a card (vp): the 更改配置 entry. I2 let it stretch
 * the whole card, which reads as the page's primary action when it is really a
 * secondary edit entry; the review's §3.3 sized it at ~240, which is what a
 * left-aligned card control should be.
 */
export const BUTTON_INLINE_WIDTH_VP: number = 240;

// ---------------------------------------------------------------------------
// 0.0.39 (I4) — the peer DETAIL pane's width cap
// ---------------------------------------------------------------------------
/**
 * The peer detail's content column cap (vp). The pane's own 1200 vp indent limit
 * is for the PANE; inside it the detail card sits beside the list column, so on a
 * very wide window the card can still be 1700 vp of prose. 720 is the readable
 * cap, centred by the detail Scroll's own alignment.
 */
export const DETAIL_CONTENT_MAX_WIDTH_VP: number = 720;

// ---------------------------------------------------------------------------
// 0.0.38 (I3) — the RAIL's connection region
// ---------------------------------------------------------------------------
/**
 * The share of the rail the connection region may take before it starts
 * scrolling (%). The region is CONTENT-height; this cap is what keeps the
 * navigation group on screen when the region's content is taller than the rail (a
 * short window), instead of pushing it out of the bottom.
 */
export const RAIL_CONNECTION_MAX_HEIGHT_PCT: number = 60;

// ---------------------------------------------------------------------------
// 0.0.39 (I4) — the 连接概览 pane (the review's §1.4 content spec)
// ---------------------------------------------------------------------------
// The pane is the answer to "the pane, which is ~60% of the window, is empty on a
// fresh connect" (§1.1) — WITHOUT duplicating the rail: rail = control + verdict
// (the state dot, the connect command), pane = the numbers the rail cannot hold.
// So the pane's state-verdict row that I3 shipped is GONE (the rail's status card
// already carries it) and what remains is exactly the review's five items: the
// hero (own NetBird IP + 域名), the three stat tiles (已连接 / 直连 / 中继), the
// relay actually in use, up to five offline peers, and the last error when one is
// recorded.
//
// Every item is still policy-gated, so the I3 rule survives: a fact the document
// did not carry produces NO row rather than a row saying 未知, and there is no
// invented zero. The one addition is the "nothing to show" row: with the pane
// reachable while the tunnel is down (an explicit 对端 choice), a pane that is
// ONLY a title would read as a broken page.
export const OV_HERO_ADDRESS: string = 'address';
export const OV_HERO_FQDN: string = 'fqdn';
export const OV_TILE_CONNECTED: string = 'connected';
export const OV_TILE_DIRECT: string = 'direct';
export const OV_TILE_RELAY: string = 'relay';
export const OV_ROW_RELAY_IN_USE: string = 'relayInUse';
export const OV_ROW_OFFLINE: string = 'offline';
export const OV_ROW_ERROR: string = 'error';
export const OV_ROW_NONE: string = 'none';
/** How many offline peers the pane lists (the review's "up to ~5"). */
export const OVERVIEW_OFFLINE_MAX_ROWS: number = 5;
/** The 连接概览 pane's stat-tile height (vp) — the review's 96 vp tile. */
export const OVERVIEW_TILE_HEIGHT_VP: number = 96;
/**
 * The pane's own content cap (vp). The tiles and the offline rows are a readable
 * column, not a stretched one; 720 is the same cap the peer detail card carries.
 */
export const OVERVIEW_CONTENT_MAX_WIDTH_VP: number = 720;

export interface OverviewFacts {
  /** The document carries the device's own overlay address. */
  hasAddress: boolean;
  /** The document carries the device's own fqdn. */
  hasFqdn: boolean;
  /** How many peers the document lists (0 ⇒ no tiles at all). */
  peerCount: number;
  /** How many of them management itself lists as offline. */
  offlineCount: number;
  /** A relay session is actually in use (the document carries its URL). */
  hasRelayInUse: boolean;
  /** A last error line exists to show. */
  hasError: boolean;
}

/**
 * The 连接概览 rows that exist for these facts, in render order. There is no
 * 连接时长 row and there will not be one: the document carries no connected-since
 * stamp (only the connector process's own `started_at_unix`, which is an uptime,
 * not a session), and the rule is to omit what is not there rather than to invent
 * it.
 */
export function overviewRowKeys(facts: OverviewFacts): string[] {
  const keys: string[] = [];
  if (facts.hasAddress) {
    keys.push(OV_HERO_ADDRESS);
  }
  if (facts.hasFqdn) {
    keys.push(OV_HERO_FQDN);
  }
  if (facts.peerCount > 0) {
    keys.push(OV_TILE_CONNECTED);
    keys.push(OV_TILE_DIRECT);
    keys.push(OV_TILE_RELAY);
  }
  if (facts.hasRelayInUse) {
    keys.push(OV_ROW_RELAY_IN_USE);
  }
  if (facts.offlineCount > 0) {
    keys.push(OV_ROW_OFFLINE);
  }
  if (facts.hasError) {
    keys.push(OV_ROW_ERROR);
  }
  if (keys.length === 0) {
    keys.push(OV_ROW_NONE);
  }
  return keys;
}

// ---------------------------------------------------------------------------
// 0.0.39 (I4) — the peer detail's FOUR SECTIONS
// ---------------------------------------------------------------------------
// The detail pane stays LAYERED (I3's B): the rows the user came for render open
// by default, everything that exists for diagnosis stays behind a collapsed
// header. The review's §3.2/§P1.7 then asks for the flat row stream to be GROUPED
// with the existing DetailSectionHeader, and names the four groups itself:
//
//   连接    the session the user came for (NetBird IP / 状态 / 连接方式 / 最近握手 /
//           发送 / 接收) — open by default, and the only group that is
//   路径    the endpoints and the bearer (对端端点 / 本机出口 / 对端网络 / 承载 /
//           中继通道地址 / 本机中继会话)
//   ICE     the candidate-path detail (6 rows; the 说明 caveat renders as a 14 vp
//           caption UNDER the group rather than as a row of its own)
//   标识    the peer's identity (公钥, rendered in the mono face so a 64-character
//           key reads as one unit)
//
// The classification is a property of the ROW, so it stays ONE classifier over the
// row's label and ONE row source in the page — never four hand-kept lists that can
// drift apart. An UNCLASSIFIED label lands in 路径, the fail-safe direction: a row
// nobody classified is far more likely to be a diagnostic than a user-facing fact,
// so it hides behind a collapsed header instead of leaking ICE/relay jargon into
// the default view.
export const PEER_DETAIL_CONNECTION: string = 'connection';
export const PEER_DETAIL_PATH: string = 'path';
export const PEER_DETAIL_ICE: string = 'ice';
export const PEER_DETAIL_IDENTITY: string = 'identity';
/** The section shown WITHOUT expanding anything: the session itself. */
export const PEER_DETAIL_CONNECTION_LABELS: string[] = [
  '域名',
  'NetBird IP',
  '状态',
  '连接方式',
  '最近握手',
  '发送',
  '接收'
];
/** The endpoints / bearer section (collapsed by default). */
export const PEER_DETAIL_PATH_LABELS: string[] = [
  '对端端点',
  '本机出口',
  '对端网络',
  '承载',
  '中继通道地址',
  '本机中继会话'
];
/** The ICE candidate-path section (collapsed by default). */
export const PEER_DETAIL_ICE_LABELS: string[] = [
  'ICE',
  'ICE 状态',
  'ICE 角色',
  'ICE 本端',
  'ICE 对端',
  'ICE 可达性',
  'ICE 最近错误'
];
/** The identity section: the public key, in mono (collapsed by default). */
export const PEER_DETAIL_IDENTITY_LABELS: string[] = [
  '公钥'
];

export function peerDetailSectionOf(label: string): string {
  for (let i = 0; i < PEER_DETAIL_CONNECTION_LABELS.length; i++) {
    if (label === PEER_DETAIL_CONNECTION_LABELS[i]) {
      return PEER_DETAIL_CONNECTION;
    }
  }
  for (let i = 0; i < PEER_DETAIL_ICE_LABELS.length; i++) {
    if (label === PEER_DETAIL_ICE_LABELS[i]) {
      return PEER_DETAIL_ICE;
    }
  }
  for (let i = 0; i < PEER_DETAIL_IDENTITY_LABELS.length; i++) {
    if (label === PEER_DETAIL_IDENTITY_LABELS[i]) {
      return PEER_DETAIL_IDENTITY;
    }
  }
  return PEER_DETAIL_PATH;
}

/**
 * The review's "drop the 域名 row when it duplicates the title" rule: the card's
 * title is the peer's display NAME, and management's fqdn is what that name is
 * built from — so when the two are the same string the row repeats the 20 vp
 * heading 8 vp below it. A fqdn that says something the title does not (a peer
 * renamed by hand) keeps its row.
 */
export function peerFqdnDuplicatesTitle(name: string, fqdn: string): boolean {
  return fqdn.length > 0 && fqdn === name;
}
