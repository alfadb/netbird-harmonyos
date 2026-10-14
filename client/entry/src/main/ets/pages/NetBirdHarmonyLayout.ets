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
