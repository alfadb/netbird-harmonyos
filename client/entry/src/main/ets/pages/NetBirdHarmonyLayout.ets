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
// THE SPLIT'S WIDTH MATH (this is the k3 1.3 correction, resolved with numbers)
// ---------------------------------------------------------------------------
// At the split threshold the window must fit the rail AND the pane's own
// Navigation, or NavigationMode.Auto stays in Stack mode and the peer detail
// blanks the list again — the exact defect this design exists to kill:
//
//   RAIL_WIDTH_VP (240) + NAV_AUTO_SPLIT_VP (600) = 840 = HARMONY_BP_LG_VP
//
// i.e. at the very bottom of `lg` the pane is EXACTLY 600 vp, which is exactly
// NavigationMode.Auto's documented threshold ("If the window width is greater
// than or equal to 600 vp, the Split mode is used ... 600 vp = minNavBarWidth
// (240 vp) + minContentWidth (360 vp)", navigation.d.ts NavigationMode.Auto). So
// no part of `lg` runs the stack-in-pane fallback, and no second threshold has to
// be invented. The arithmetic is exact, not a near miss: 240 and 840 are integers
// and a fixed 240 vp rail leaves 840 - 240 = 600 vp exactly, so the framework's
// own `>= 600` comparison cannot lose to a rounding error.
//
// k3's suggested 300 / 280 / 320 would need 300 + 280 + 320 = 900 vp, so at a
// 840 vp window the pane would still be smaller than the pane Navigation's own
// minimum and the detail WOULD blank the list — so those numbers are not used.
// The rail is 240 vp (the official minNavBarWidth default) for that reason.
//
// The xl indent-limit (content maxWidth) and a roomier rail at xl are NOT in this
// increment on purpose: a second rail width would make the rail jump while a 2in1
// window is dragged across 1440 vp.

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

// ---- the split branch's width budget (all vp) -------------------------------
/**
 * The persistent left column. 240 vp = the official `minNavBarWidth` default, and
 * it is what makes 840 = 240 + 600 work at the bottom of `lg`.
 */
export const RAIL_WIDTH_VP: number = 240;
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
 * the official `lg` boundary and the width the rail plus the pane's Navigation
 * really need (RAIL_WIDTH_VP + NAV_AUTO_SPLIT_VP). With the numbers above both are
 * 840, which is the point: the shell's threshold and the pane's own split
 * threshold coincide, so the bottom of `lg` is inside the split rather than
 * outside it.
 */
export function splitThresholdVp(): number {
  const needed: number = RAIL_WIDTH_VP + NAV_AUTO_SPLIT_VP;
  return needed > SPLIT_MIN_WINDOW_VP ? needed : SPLIT_MIN_WINDOW_VP;
}

/**
 * `'split'` (rail + pane) or `'single'` (today's bottom-tab shell) for a window
 * width in vp.
 */
export function paneLayoutOf(widthVp: number): string {
  return widthVp >= splitThresholdVp() ? PANE_SPLIT : PANE_SINGLE;
}
