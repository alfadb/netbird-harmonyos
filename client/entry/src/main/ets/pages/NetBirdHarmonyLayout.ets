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
// 0.0.40 (I5): the independent visual review (pc-ui-design/ASTRA-REVIEW-I4.md)
// read the I4 build and its three 必须改 items each moved a number that already
// lived here — the rail's traffic summary becomes an OMITTED row when the
// document carries no traffic (and a bound-then-stacked pair when it does, so no
// counter can ever be ellipsised), the connection region's 60 % height cap
// becomes a HEIGHT BUDGET subtracted from the rail's own measured height (the
// cap clipped the 详细状态 entry while 132 px of slack sat under the card), and
// the peer row gains the review's own selected state (a 3 vp primary bar + a
// selected-row tint, both of which come out of the row's text budget). The
// chevron slot and the guide pane's block width/offset are the same kind of
// number. Every one of them is asserted by run-harmony-layout.mts.
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
// 0.0.43 (I8): the peer row's own three owner findings — the selected row's tint
// becomes an INSET ROUNDED BLOCK (inset = the list content padding, radius stated
// and bounded against the card's own 16), the 直连/中继 chip and the status dot
// leave line 1 for a FULL-HEIGHT META COLUMN the row body centres vertically (with
// the compact tier's impossibility stated as arithmetic, since its name floor
// cannot pay for such a column), and the row gains a TIER-GATED copy slot for the
// hostname and the IP. The meta column's 112 vp budget is replaced by the sum of
// its own parts (98), because the column now has to be exactly as wide as the
// chrome it holds.
//
// 0.0.44 (I9): the detail row adopts the official desktop client's shape — a leading
// semantic glyph, the label, the value, and the copy mark when the value is an
// identifier or an address. The DECISIONS are here: a row's FIELD KIND (its semantic
// identity), the glyph that kind draws, and whether that kind's value may be copied.
// All three are pure functions over the row's label, so "every row of the pane has a
// glyph" and "exactly the owner's six identifier/address rows are copyable" are
// host-asserted over the whole rendered label set. The row's value budget is here
// too, because the glyphs are paid for by the row's WIDTH and that arithmetic is
// what says no value can be ellipsised by them.
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
//
// ---------------------------------------------------------------------------
// 0.0.46 (I11) — THE UNIT IS fp, AND THE APP NOW FOLLOWS THE SYSTEM SCALE
// ---------------------------------------------------------------------------
// These six numbers are FONT sizes, so they are fp (font pixels) and NOT vp: a
// plain number handed to `fontSize()` resolves to fp (local SDK,
// ets/component/text.d.ts:146 — "If fontSize is of the number type, the unit fp is
// used"), and fp is the unit that grows with the system font-size setting. They were
// named `_VP` while this app pinned the scale at 1 (where 1 fp = 1 vp), and that is
// why 设置 → 显示和亮度 → 字体大小 had NO effect on the app at all: a HarmonyOS
// application does not follow the system font size unless AppScope/app.json5
// declares `"configuration": "$profile:configuration"` and that profile sets
// `configuration.fontSizeScale = "followSystem"` (OpenHarmony app.json5 reference,
// `configuration` tag — initial value `nonFollowSystem`; schema:
// sdk/default/openharmony/toolchains/modulecheck/configuration.json). The app
// shipped neither, so every fp size rendered at 1 fp = 1 vp, i.e. fixed. 0.0.46
// adds both and keeps the scale's RELATIVE values verbatim: only the unit — and
// therefore the name — changes.
//
// WHAT BOUNDS THE FOLLOWED SCALE. The system's 档位 are 1 / 1.15 / 1.3 / 1.45 /
// 1.75 / 2 / 3.2 (ArkUI 适老化 guide "各档位对应参数"), and three ceilings bound
// this app:
//
//   * TYPE_SCALE_MAX_APP (1.75) — the app-level ceiling in
//     AppScope/resources/base/profile/configuration.json (`fontSizeMaxScale`). The
//     适老化 guide's own recommendation is "最大应调整至 1.75 倍，但部分组件可调整至 2
//     倍"; text in an auto-height container takes it as-is (no attribute).
//   * TYPE_SCALE_MAX_ROW (1.3) — the ceiling every text inside a fixed-height row,
//     a fixed-height tile or a width-bounded row (the peer row's name/IP, the
//     detail row's value, the rail's copied value) carries as its own
//     `.maxFontScale(...)`. It is DERIVED, not chosen: at the narrowest split tier
//     (840 vp) the peer row's IP line is 133 vp wide (peerRowIpWidthFor) while a
//     dotted quad at the caption size needs 98 vp at 1x, so the largest 档位 that
//     still renders the address on ONE line is 1.3 (98 x 1.3 = 127.4 <= 133; 1.45
//     already needs 142 vp). The same cap keeps the 64-character mono public key
//     inside the detail row's DETAIL_ROW_VALUE_MAX_LINES at the narrowest pane (7
//     lines of the 8 allowed) and keeps every 36…48 vp fixed row holding its tallest
//     line box (16 fp x 1.3 x 1.5 em = 31.2 <= 36 — see typeFitsFixedHeightVp).
//   * TYPE_SCALE_MAX_DISC (1) — the connect disc's label. The disc is a fixed
//     104/132 vp circle and its widest label (正在断开…: 4 CJK glyphs + the
//     full-width ellipsis = 5 em) already spends 110 of the disc's 126 vp inner
//     width at 22 fp on the narrow shell (90 of 98 at 18 fp on the wide one), so any
//     scale above 1 clips it. That is also the platform's own convention for a fixed
//     circular control: `Button.maxFontScale` (local SDK
//     ets/component/button.d.ts:775, @since 18) states that a circular button's
//     default maximum scale is 1. The disc therefore follows the 适老化 guide's ICON
//     rule ("图标及图片不会随着字体的变大而变化") rather than its text rule — the
//     label is part of the control's geometry.
//
// `typeLineBoxVp` / `typeFitsFixedHeightVp` below are the arithmetic the host check
// uses to hold those claims; the page reads only the three caps.
/** The app title inside the rail ("NetBird"). */
export const TYPE_APP_TITLE_FP: number = 22;
/** A page/section headline (对端, 设置, 对端详情, the guide's state). */
export const TYPE_PAGE_TITLE_FP: number = 20;
/** A settings group header above its cards. */
export const TYPE_SECTION_TITLE_FP: number = 16;
/** Body text and the primary value of a label/value row. */
export const TYPE_BODY_FP: number = 15;
/** Secondary/caption text: ≥ 12 vp everywhere, ≥ 14 vp on the PC (this value). */
export const TYPE_CAPTION_FP: number = 14;
/** The 连接概览 pane's stat-tile number (the review's 24 vp Medium tile figure). */
export const TYPE_STAT_VALUE_FP: number = 24;
/**
 * A row's disclosure mark ('›', '▾'/'▸'): 18 fp, on the same scale as the row's
 * label. It used to be a bare `18` literal at five call sites.
 */
export const TYPE_CHEVRON_FP: number = 18;

// ---- the three ceilings (0.0.46) -------------------------------------------
/** The app's ceiling — AppScope/resources/base/profile/configuration.json. */
export const TYPE_SCALE_MAX_APP: number = 1.75;
/** The ceiling of any text in a fixed-height or width-bounded row (see above). */
export const TYPE_SCALE_MAX_ROW: number = 1.3;
/** The connect disc's label ceiling (1 = the platform's circular-button default). */
export const TYPE_SCALE_MAX_DISC: number = 1;

/**
 * The line box (vp) a `sizeFp` run occupies at `scale`: 1.5 em, the rule this file
 * already uses for the copy slot versus a caption line (`PEER_ROW_COPY_SLOT_VP <=
 * TYPE_CAPTION_FP * 1.5`). 1 fp is 1 vp at scale 1, so this is the height a row
 * really has to hold once the system scale is applied.
 */
export function typeLineBoxVp(sizeFp: number, scale: number): number {
  return sizeFp * scale * 1.5;
}

/**
 * Does a fixed-height container of `heightVp` still hold the `sizeFp` line box at
 * `scale`? This is the question every `.height(36…48)` row in the page answers with
 * `TYPE_SCALE_MAX_ROW`, and the host check sweeps it for the shortest of them.
 */
export function typeFitsFixedHeightVp(heightVp: number, sizeFp: number,
  scale: number): boolean {
  return heightVp >= typeLineBoxVp(sizeFp, scale);
}

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

// ---- the connect control's state ring (0.0.39 (I4), widened by I5) ----------
// The review's §4.3: the 104 vp disc painted the CURRENT STATE as its fill while
// its label is the INVERSE ACTION (断开 on green), so it read as a lamp and it
// outranked the status card it exists to support. Fill = the primary action role
// always; the state moves to this ring in the state colour.
//
// 0.0.40 (I5) §7: the ASTRA review could barely SEE that ring ("圆盘状态环略薄")
// — 2 vp of state colour on the rim of a 104 vp disc is 2 % of the silhouette.
// 3 vp doubles the painted area without touching the disc's size, its label or
// its fill; the review's alternative (a 2 vp dark gap between disc and ring)
// needs a Stack layer and would shrink the disc, which §7 explicitly forbids
// ("保持圆盘尺寸与文字"), so it is deliberately NOT taken.
/** The connect disc's state ring (vp). */
export const CONNECT_RING_WIDTH_VP: number = 3;

// ---- the connect disc's own geometry and its label ceiling (0.0.46) ---------
/** The disc's size (vp) on the wide (split) shell — the page's connectButtonSize(). */
export const CONNECT_DISC_WIDE_SIZE_VP: number = 104;
/** The disc's label size (fp) on the wide shell — the page's connectButtonFontSize(). */
export const CONNECT_DISC_WIDE_FONT_FP: number = 18;
/** The disc's size (vp) on the narrow shell (byte-identical to the pre-I2 phone). */
export const CONNECT_DISC_NARROW_SIZE_VP: number = 132;
/** The disc's label size (fp) on the narrow shell. */
export const CONNECT_DISC_NARROW_FONT_FP: number = 22;
/**
 * The widest label the disc ever draws, in em: 正在断开… (and 连接中…) — four CJK
 * glyphs at 1 em each plus the full-width ellipsis.
 */
export const CONNECT_LABEL_WIDEST_EM: number = 5;
/**
 * Does the disc's INNER width (the box less the ring, which ArkUI draws inside it)
 * still hold that label at `scale`? False for both shells at 1.15 (the next system
 * 档位), which is why TYPE_SCALE_MAX_DISC is 1 — see the scale note at the top.
 */
export function connectLabelFitsDiscVp(sizeVp: number, sizeFp: number,
  scale: number): boolean {
  return sizeFp * CONNECT_LABEL_WIDEST_EM * scale <= sizeVp - 2 * CONNECT_RING_WIDTH_VP;
}

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

// 0.0.43 (I8) §3 — THE RAIL'S COPY SLOT.
// The rail's local NetBird IP and 域名 rows carry their copy icon ALWAYS (the
// owner's visibility rule: no hover is needed on the identity card), so the row's
// VALUE text and the icon share the value column: the text is what has to fit
// beside a slot, and `railInfoValueTextWidthVp` is that width. The slot is the peer
// row's own 16 vp glyph — one icon, one size, in every surface that copies a value
// (the peer row's slot is the same number for the same reason: a slot no taller
// than a text line cannot change any of these rows' heights). The selfcheck holds
// the remaining text against the SAME dotted-quad floor the peer list uses — the
// widest thing the PC rail ever puts in a copied row is an IP.
/** The rail's copy slot (vp) — the peer list's own glyph, one icon across the app. */
export const RAIL_INFO_COPY_SLOT_VP: number = 16;

/**
 * The rail row's VALUE TEXT width (vp) when the row carries the copy icon — the
 * value column less the mark's slot AND (0.0.45 (I10)) the 5 vp gap that now puts
 * the mark immediately after the value's own glyphs. The caller passes a WINDOW
 * width and the rail tier is read from it, so this is the split shell's form; the
 * narrow branch's card is the page (railInfoValueTextWidthFor).
 */
export function railInfoValueTextWidthVp(widthVp: number): number {
  return railInfoValueWidthVp(widthVp) - RAIL_INFO_COPY_SLOT_VP - COPY_ICON_GAP_VP;
}

/**
 * The rail (or narrow-branch home card) row's value-text cap (vp) FOR A WINDOW: the
 * value column at that window's own width — the tier's rail on the split shell,
 * the page on the narrow branch — less the copy slot and its gap.
 *
 * The page needs this form and not railInfoValueTextWidthVp because `homeTab` is
 * ONE builder on both branches: inside the rail its card is `railWidthFor(window)`
 * wide, and on the phone the same card is the page's whole width. Feeding the rail
 * tier's 240–320 vp column to the phone would ellipsise a 28-character 域名 that has
 * 500 vp of room beside it.
 */
export function railInfoValueTextWidthFor(windowVp: number): number {
  const page: number = paneLayoutOf(windowVp) === PANE_SPLIT
    ? railWidthFor(windowVp) : windowVp;
  const width: number = page - 2 * RAIL_PAGE_PADDING_VP -
    2 * RAIL_CARD_PADDING_VP - RAIL_INFO_COPY_SLOT_VP - COPY_ICON_GAP_VP;
  return width > 0 ? width : 0;
}

/**
 * 0.0.46 (I11) — DOES THE RAIL'S COPIED VALUE COLUMN STILL HOLD A DOTTED QUAD AT THE
 * ROW CAP? The rail's rows are the identity card's NetBird IP and 域名, so the
 * widest thing they ever render is an address; at TYPE_SCALE_MAX_ROW the address's
 * advance grows by that factor while the column does not (the rail is a fixed 240…
 * 320 vp), so this is the statement that keeps "the rail never ellipsises an IP"
 * true on a large system font as well as on the standard one. The value is the body
 * size, so it pays the body advance (0.5 em), not the caption's.
 */
export function railInfoValueHoldsIpAtCap(windowVp: number): boolean {
  return railInfoValueTextWidthVp(windowVp) >=
    DETAIL_ROW_VALUE_CHARS * (TYPE_BODY_FP / 2) * TYPE_SCALE_MAX_ROW;
}

// ---------------------------------------------------------------------------
// 0.0.40 (I5) §1 — the rail's traffic summary: OMIT the zero, bound the value
// ---------------------------------------------------------------------------
// The ASTRA review's §1 read the rail card's traffic line as
// "发送 0 B（0 字节） · 接收 0 …" — two defects at once: a row that exists only to
// say zero, and an ellipsis at the 240/280 vp tiers because `byteText` appends
// the exact byte count in brackets ("0 B（0 字节）" is ~3x the width of "0 B").
//
// The rule here is the review's first option, and it is a POLICY over the two
// counters, not a string length check:
//
//   * no counter above zero (or no counters at all — a document with no wg
//     section) ⇒ the row does not exist. "发送 0 B · 接收 0 B" is not information;
//     it is a row the user has to read to learn nothing, and on a disconnected
//     rail it is what pushed the card into its ceiling. One direction being zero
//     while the other is not still renders BOTH: "接收 0 B" is then a real fact
//     (nothing has arrived), not a placeholder.
//   * when the row DOES exist its value is bounded, so it never needs an
//     ellipsis: the pair is rendered as two stacked lines (发 X / 收 Y), each of
//     which is "the 发/收 prefix + the widest formatBytes() output". The review's
//     own §1 fallback ("240 vp 档或较长数值放不下时拆成「发送 / 接收」两行，行高约
//     4 vp，数值不得省略"), applied at every tier instead of only the narrow one —
//     one shape, and the arithmetic below is the proof that neither line can
//     overflow at any rail width.
/** The widest `formatBytes()` output ("1023.9 PB"): 9 glyphs. */
export const RAIL_TRAFFIC_VALUE_CHARS: number = 9;
/** One glyph of that value at the body size (0.5 em, proportional-Latin rule). */
export const RAIL_TRAFFIC_CHAR_VP: number = TYPE_BODY_FP * 0.5;
/** The 发/收 prefix and its space (one CJK glyph at the body size, plus 4 vp). */
export const RAIL_TRAFFIC_PREFIX_VP: number = TYPE_BODY_FP + 4;
/** The width (vp) one stacked traffic line can need — the row's own ceiling. */
export function railTrafficLineWidthVp(): number {
  return RAIL_TRAFFIC_PREFIX_VP + RAIL_TRAFFIC_VALUE_CHARS * RAIL_TRAFFIC_CHAR_VP;
}

/**
 * Does the rail render a traffic summary at all? True exactly when the document
 * carries counters AND at least one of them is above zero. A non-finite or
 * missing number is "no traffic", the same direction `formatBytes` fails in
 * (未知, never a fake zero).
 */
export function railTrafficVisible(txBytes: number, rxBytes: number): boolean {
  const sent: boolean = Number.isFinite(txBytes) && txBytes > 0;
  const received: boolean = Number.isFinite(rxBytes) && rxBytes > 0;
  return sent || received;
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
// 0.0.43 (I8) §2 — THE META IS A COLUMN OF ITS OWN PARTS.
// ---------------------------------------------------------------------------
// Up to 0.0.42 the 直连/中继 chip, the 空闲/离线 text and the status dot were
// children of LINE 1's Row, so the row's chrome budget was a single number with
// slack in it (112 vp) and the three of them sat on the name's baseline while the
// row was two lines tall — the owner's I8 finding: "the chip and the status dot
// currently align with the first text line and read as top-heavy".
//
// I8 makes them a trailing column of the row body (see peerRowMetaWidthVp), which
// is vertically centred against the two-line text block by the row's own
// `VerticalAlign.Center` — so the column has to know its EXACT width, and the
// budget is derived from the four widths the chrome actually needs:
//
//   chip        中继 / 直连 — two CJK glyphs at 14 vp + the pill's 8 + 8 vp padding
//   status      空闲 / 离线 — two CJK glyphs at 14 vp
//   dot         the 10 vp status circle
//   gap         the row's own child gap (Row({ space: 8 }))
//
// `chip + gap + status + gap + dot` = 98 replaces the 112 (+14 vp of slack), and
// the same four parts reproduce the COMPACT tier's two budgets exactly as the
// tier renders them (the dot's column on line 1; the chip + status beside the IP).
/** Line 1's / the column's chip width (vp): 中继 / 直连 at 14 vp + the pill's padding. */
export const PEER_ROW_CHIP_WIDTH_VP: number = 44;
/** The 空闲 / 离线 text's width (vp) at 14 vp. */
export const PEER_ROW_STATUS_TEXT_WIDTH_VP: number = 28;
/** The status dot's diameter (vp). */
export const PEER_ROW_DOT_VP: number = 10;
/** The peer row's own child gap (vp) — the `space` of every Row in the row body. */
export const PEER_ROW_META_GAP_VP: number = 8;
/**
 * The WIDE tiers' (medium/roomy) full-height meta column: the 直连/中继 chip, the
 * 空闲/离线 status text and the status dot, with their gaps — the whole trailing
 * column the I8 row centres vertically. The 已连接 TEXT no longer renders at all
 * (the dot and the chip already say it twice — the review's §3.1), but the column
 * keeps room for the longest of the other two states, so a row that shows 离线
 * cannot push the name below its floor.
 */
export const PEER_ROW_META_WIDE_VP: number =
  PEER_ROW_CHIP_WIDTH_VP + PEER_ROW_META_GAP_VP + PEER_ROW_STATUS_TEXT_WIDTH_VP +
    PEER_ROW_META_GAP_VP + PEER_ROW_DOT_VP;
/**
 * Line 1's right-hand block at the COMPACT tier: the status dot and its gap only.
 * The chip and any 空闲/离线 text ride line 2 (see PEER_ROW_L2_META_COMPACT_VP),
 * because that tier's 28-character name floor cannot pay for a wider meta column
 * (see the I8 note on peerRowMetaWidthVp).
 */
export const PEER_ROW_META_COMPACT_VP: number =
  PEER_ROW_DOT_VP + PEER_ROW_META_GAP_VP;
/**
 * Line 2's right-hand block at the COMPACT tier: the 直连/中继 chip, the
 * 空闲/离线 text and their gaps, beside the IP. Line 2's text width minus this is
 * still >= the IP's own one-line floor at every window of the shell.
 */
export const PEER_ROW_L2_META_COMPACT_VP: number =
  PEER_ROW_META_GAP_VP + PEER_ROW_CHIP_WIDTH_VP + PEER_ROW_META_GAP_VP +
    PEER_ROW_STATUS_TEXT_WIDTH_VP;
/** The floor line 2 needs for a dotted-quad IPv4 on ONE line at caption size. */
export const PEER_ROW_IP_MIN_WIDTH_VP: number = 120;
/**
 * 0.0.40 (I5) §3 — the SELECTED row's left marker (vp). The ASTRA review's §3 read
 * the I4 对端详情 frame and could not tell the row whose detail was open
 * (`blkserver.netcenter.local`) from its neighbours: "该行底色、文字和边缘与相邻
 * 行没有可辨认的选中区别". The marker is the same 3 vp primary bar the rail's own
 * destination rows carry (RAIL_NAV_BAR_WIDTH_VP), so the "this is the current
 * one" language is one language in the app, and it is part of the row's text
 * budget below — a marker that overlaps the name would be paid for by the name.
 */
export const PEER_ROW_SELECT_BAR_VP: number = RAIL_NAV_BAR_WIDTH_VP;
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
// 0.0.41 (I6) §3: PEER_SEARCH_MAX_WIDTH_VP (320) is RETIRED. The search field is
// the list card's own width now (peerListCardWidthVp below) — see that function.

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
 * The peer LIST CARD's width (vp): the list column minus the page padding — the
 * width the card's own `.width('100%')` resolves to inside the padded column.
 *
 * 0.0.41 (I6) §3 made this the ONE number behind two things that used to disagree:
 * the row-text budget below was always measured from it, while the search field
 * was additionally capped at a PEER_SEARCH_MAX_WIDTH_VP of 320 — in the owner's
 * 0.0.40 frame the field therefore ended ~34 vp short of the card on EACH side
 * ("搜索框比下面的对端列表窄"). The cap is gone and both callers read this
 * function, so the field and the list have the same left and right edge by
 * construction rather than by arithmetic that happens to agree.
 */
export function peerListCardWidthVp(navBarWidthVp: number): number {
  const full: number = navBarWidthVp - 2 * RAIL_PAGE_PADDING_VP;
  return full > 0 ? full : 0;
}

/**
 * The width (vp) one peer row's TEXT has: the list card's width minus the card's
 * own padding and — 0.0.40 (I5) §3 — the selected row's own left marker, which
 * every row reserves (transparent when unselected, so selecting a peer does not
 * shift the text by 3 vp). Line 2 (the NetBird IP) owns this full width, so
 * `>= PEER_ROW_IP_MIN_WIDTH_VP` is exactly "the IP never wraps".
 */
export function peerRowTextWidthVp(navBarWidthVp: number): number {
  return peerListCardWidthVp(navBarWidthVp) - 2 * PEER_ROW_LIST_PADDING_VP -
    PEER_ROW_SELECT_BAR_VP;
}

/**
 * The peer LIST CARD's width (vp) at a window's OWN branch: the split shell's list
 * column — a function of the rail tier table (peerListNavBarWidthFor) — or, on the
 * narrow branch, the PAGE, because peerListBody is the phone's 对端 tab body and its
 * card is `width 100%` inside a page padded by RAIL_PAGE_PADDING_VP.
 *
 * 0.0.45 (I10) is why this exists. I8/I9 only ever READ these budgets in the
 * selfcheck: the rows' text was `.layoutWeight(1)`, so the RUNTIME ellipsis width was
 * whatever the row's layout gave it. I10 makes the text content-sized, so the budget
 * is now the runtime cap on every branch — and feeding the narrow branch the split
 * shell's `peerListNavBarWidthFor(window)` column (240 vp, i.e. 83 vp of name) cut
 * the phone's IPs to `100.108.2…` on a row that has ~340 vp of text width. Never
 * negative.
 */
export function peerRowCardWidthFor(windowVp: number): number {
  const card: number = paneLayoutOf(windowVp) === PANE_SPLIT
    ? peerListCardWidthVp(peerListNavBarWidthFor(windowVp))
    : windowVp - 2 * RAIL_PAGE_PADDING_VP;
  return card > 0 ? card : 0;
}

/**
 * The width (vp) one peer row's TEXT has at a window's own branch: the card at that
 * branch (peerRowCardWidthFor) less the row's own content padding and the selection
 * marker every row reserves. The split form is peerRowTextWidthVp, unchanged.
 */
export function peerRowTextWidthFor(windowVp: number): number {
  const text: number = peerRowCardWidthFor(windowVp) - 2 * PEER_ROW_LIST_PADDING_VP -
    PEER_ROW_SELECT_BAR_VP;
  return text > 0 ? text : 0;
}

/**
 * The row's right-hand META block (vp) at a window's tier — see the constants.
 *
 * 0.0.43 (I8) §2 — IT IS ALSO WHERE THE BLOCK LIVES, and that is why it is called
 * a column now. At the medium/roomy tiers the chip + status text + dot become ONE
 * trailing column of the row body, a SIBLING of the two-line text block, and the
 * row body's `VerticalAlign.Center` centres it against BOTH lines — which is the
 * owner's "center them vertically within the row's full height". At the compact
 * split tier (840…1023 vp) the same thing is geometrically impossible and the
 * budget shows why:
 *
 *   the tier's row has text width `T` and has to hold, at 840 vp, T = 221 vp
 *     name   >= PEER_ROW_NAME_MIN_WIDTH_VP (196)   — a real 28-char fqdn
 *     IP     >= PEER_ROW_IP_MIN_WIDTH_VP   (120)   — a dotted quad on one line
 *   and a full-height column takes its width out of BOTH lines, so the column can
 *   be at most `T - 196` = 25 vp wide while line 2 additionally needs its chip +
 *   status (88) — 25 + 88 + 120 = 233 > 221 vp. The tier therefore keeps I4's
 *   condensation (the dot on line 1, the chip + status beside the IP on line 2),
 *   and the selfcheck asserts BOTH the impossibility and the shipped arrangement.
 */
export function peerRowMetaWidthVp(windowVp: number): number {
  return peerRowCondensed(windowVp) ? PEER_ROW_META_COMPACT_VP : PEER_ROW_META_WIDE_VP;
}

// ---------------------------------------------------------------------------
// 0.0.43 (I8) §3 — the row's COPY slot
// ---------------------------------------------------------------------------
// The list rows carry a copy affordance beside the hostname and beside the IP, and
// it is INVISIBLE until the row is hovered or selected (I8's own visibility rule;
// the phone branch has no pointer at all and keeps its rows unchanged).
//
// TWO things have to be true for that to be invisible in the LAYOUT as well, and
// the 0.0.43 device frame caught the first version of this code getting the second
// one wrong:
//
//   1. the slot is created for the WHOLE TIER, not for the hovered row. The row
//      renders the component whenever `peerRowCopyVisible(window)` holds and only
//      its `visible` @Prop follows the pointer — because an `if` around the node
//      removes it from the layout when the pointer leaves, which SHORTENS the row:
//      the frame measured the hovered and the selected rows at 74 vp against their
//      neighbours' 64 (96 px vs 83 px pitch at 1.2964 px/vp), i.e. hovering a row
//      pushed the whole list below it down. That is the reflow this slot exists to
//      prevent, so the gate is the tier.
//   2. the slot is no TALLER than the row's own text lines, so reserving it cannot
//      grow the 64 vp row either: it is exactly the 16 vp glyph, which is under
//      both the name's 15 vp line box and the IP's 14 vp one. Invisible is
//      `Visibility.Hidden` (component/enums.d.ts:1851 — "hidden, and a placeholder
//      is used for it in the layout"), never `Visibility.None`.
//
// The slot is also TIER-GATED, and that gate is arithmetic rather than taste: at
// the compact split tier the 28-character name floor has 7 vp of slack (203
// against 196 — see peerRowMetaWidthVp) and a 16 vp slot does not fit; from the
// medium tier up the same floor has the room. `peerRowCopyVisible` is that rule,
// and the row, the budget and the selfcheck all read it.
/** The drawn copy glyph's size (vp) — two overlapping rounded rectangles. */
export const PEER_ROW_COPY_GLYPH_VP: number = 16;
/**
 * The glyph's layout slot (vp): the glyph itself. Deliberately NOT bigger — the
 * slot is reserved on every row of the tier, so a slot taller than the text lines
 * would raise the two-line row's height (24 vp measured 74 vp rows; 16 keeps them
 * at the 64 vp the compact form states).
 */
export const PEER_ROW_COPY_SLOT_VP: number = PEER_ROW_COPY_GLYPH_VP;
/**
 * The icon's TOUCH target (vp): >= 40 x 40 even though the glyph is 16, via the
 * component's `responseRegion` (component/common.d.ts:17311, public since API 8).
 */
export const PEER_ROW_COPY_TOUCH_VP: number = 40;
// ---------------------------------------------------------------------------
// 0.0.45 (I10) — THE HUG: the gap between a value and its copy icon
// ---------------------------------------------------------------------------
// I8/I9 put the mark in every surface but let the text STRETCH to the end of its
// column, so the icon landed after the stretched text BOX rather than after the
// glyphs: the owner's frame reads "the copy icons float in a middle column", and the
// lead measured the list's icon left edge at x≈350 against a name whose ink ends at
// ≈215 (the name's box ran to the meta column). The rail's IP row had the same
// shape, and its value is left-aligned, so its mark sat at the card's right edge.
//
// The fix is structural in each row (the text is sized to its CONTENT and the
// text+mark pair is packed left) and this is the ONE number that separates them:
// the same 5 vp in the list's name line, the list's IP line, the detail row and the
// rail.
//
// WHY 5 AND NOT 8 — the acceptance measures INK to INK ("the text's right edge and
// the icon's left edge" in a zoomed frame), and three things sit between this number
// and that measurement, all of them positive:
//
//   * the mark's ink starts 1 vp inside its 16 vp slot: the two 8×10 sheets are
//     offset ∓3 vp and ArkUI draws a Column's 1.5 vp border INSIDE its box, so the
//     back sheet spans x = 1…9 and the ink is 14 vp wide inside a 16 vp box
//     (both numbers are on the device frame: 14 vp = 49 px, 16 vp slot = 56 px);
//   * a proportional face leaves the last glyph's right side bearing INSIDE the
//     text's advance, so the text's ink ends ~1.3 vp before its box does (measured
//     on the frame as ≈4.5 px at 3.5 px/vp);
//   * the 8 vp an earlier version shipped measured 28 px = 8.0 vp ink-to-ink on the
//     0.0.45 phone frame — exactly ON the band's upper edge, where any read error
//     puts it outside. 5 vp measures ≈7.3 vp, comfortably inside 4…8 vp, and still
//     reads as attached (17.5 px on that device).
/** The smallest gap (vp) the acceptance allows between a value and its mark. */
export const COPY_ICON_GAP_MIN_VP: number = 4;
/** The largest gap (vp) the acceptance allows between a value and its mark. */
export const COPY_ICON_GAP_MAX_VP: number = 8;
/** The shipped gap (vp, text box → mark box) between a value and its copy mark. */
export const COPY_ICON_GAP_VP: number = 5;

/** The gap (vp) a copy mark keeps from the text it belongs to — one number, every surface. */
export function copyIconGapVp(): number {
  return COPY_ICON_GAP_VP;
}

/** Is `gapVp` inside the shipped 4…8 vp band? (the acceptance's own question.) */
export function copyIconGapInBand(gapVp: number): boolean {
  return gapVp >= COPY_ICON_GAP_MIN_VP && gapVp <= COPY_ICON_GAP_MAX_VP;
}

/**
 * Does the peer LIST ROW render a copy slot at this window width? True exactly on
 * the split shell at the medium/roomy rail tiers — the pointer branch whose width
 * budget can afford it. The narrow (phone/tablet) branch is FALSE: it has no hover,
 * so its rows stay byte-for-byte what they were (I8's phone-branch decision), and
 * the compact split tier is false because its name floor cannot pay for the slot.
 */
export function peerRowCopyVisible(windowVp: number): boolean {
  return paneLayoutOf(windowVp) === PANE_SPLIT &&
    railTierOf(windowVp) !== RAIL_TIER_COMPACT;
}

/** The row's copy slot (vp) at a window width — 0 when the tier cannot afford it. */
export function peerRowCopySlotVp(windowVp: number): number {
  return peerRowCopyVisible(windowVp) ? PEER_ROW_COPY_SLOT_VP : 0;
}

/**
 * What the copy affordance takes out of a text BUDGET (vp) at a window width: the
 * mark's slot AND the gap that puts it 5 vp after the last glyph, or 0 where the
 * tier has no slot at all.
 *
 * 0.0.45 (I10) is the reason the gap joined the budget. The two numbers used to be
 * one (the slot alone), because the text was STRETCHED: its box ran to the meta
 * column and the icon followed the box, wherever the box ended. Now the box is the
 * glyphs, so `budget + gap + slot` is what the row really spends — and the text's
 * max width must already carry it, or a name that fills its budget would push the
 * mark into the chip. The floors are unaffected: the gap is only spent at the
 * medium/roomy tiers, where the name's floor has 60+ vp of room.
 */
export function peerRowCopyTakeVp(windowVp: number): number {
  return peerRowCopyVisible(windowVp) ? COPY_ICON_GAP_VP + PEER_ROW_COPY_SLOT_VP : 0;
}

/**
 * The width (vp) line 1's NAME has at a window width: the row's text width at that
 * window's OWN branch (peerRowTextWidthFor), minus the meta block at that window's
 * tier and minus what the copy affordance takes (peerRowCopyTakeVp: the 5 vp gap +
 * the 16 vp slot from the medium tier up, 0 at the compact split tier and 0 on the
 * narrow branch, neither of which has a mark in its rows).
 * `>= PEER_ROW_NAME_MIN_WIDTH_VP` is exactly "a real 28-character fqdn fits" — at
 * every window of the SPLIT shelf, which is the claim the selfcheck sweeps (the
 * phone's 320–839 vp page cannot hold a 28-character fqdn beside the meta column at
 * all, and never could).
 */
export function peerRowNameWidthVp(navBarWidthVp: number, windowVp: number): number {
  return peerRowTextWidthVp(navBarWidthVp) - peerRowMetaWidthVp(windowVp) -
    peerRowCopyTakeVp(windowVp);
}

/** The name column's width (vp) for a window width — the composed, branch-aware form. */
export function peerRowNameWidthFor(windowVp: number): number {
  return peerRowTextWidthFor(windowVp) - peerRowMetaWidthVp(windowVp) -
    peerRowCopyTakeVp(windowVp);
}

/**
 * The width (vp) line 2's IP has. At the compact tier the IP shares line 2 with the
 * chip + status text (I4's condensation) and the dot is on line 1, so only that
 * line's block comes off. At the wide tiers the IP shares the ROW with the
 * full-height meta column (I8 §2) and carries its own copy icon (I8 §3), so both
 * come off.
 * `>= PEER_ROW_IP_MIN_WIDTH_VP` is exactly "the IP never wraps" at every window.
 */
export function peerRowIpWidthVp(navBarWidthVp: number, windowVp: number): number {
  const text: number = peerRowTextWidthVp(navBarWidthVp);
  if (peerRowCondensed(windowVp)) {
    return text - PEER_ROW_L2_META_COMPACT_VP;
  }
  return text - peerRowMetaWidthVp(windowVp) - peerRowCopyTakeVp(windowVp);
}

/** The IP column's width (vp) for a window width — the composed, branch-aware form. */
export function peerRowIpWidthFor(windowVp: number): number {
  const text: number = peerRowTextWidthFor(windowVp);
  if (peerRowCondensed(windowVp)) {
    return text - PEER_ROW_L2_META_COMPACT_VP;
  }
  return text - peerRowMetaWidthVp(windowVp) - peerRowCopyTakeVp(windowVp);
}

/**
 * 0.0.46 (I11) — THE WIDTH THE ADDRESS NEEDS AT THE ROW CAP: a dotted quad (14
 * characters, DETAIL_ROW_VALUE_CHARS) at the caption size, 0.5 em per character
 * (PEER_ROW_NAME_CHAR_VP's rule), multiplied by TYPE_SCALE_MAX_ROW. 98 vp at 1x and
 * 127.4 vp at the cap, against the 133 vp the IP line owns at the narrowest split
 * tier (peerRowIpWidthFor(840)) — the two numbers the cap of 1.3 was derived from.
 */
export function peerRowIpOneLineAtCapVp(): number {
  return DETAIL_ROW_VALUE_CHARS * (TYPE_CAPTION_FP / 2) * TYPE_SCALE_MAX_ROW;
}

// ---------------------------------------------------------------------------
// 0.0.43 (I8) §1 — the row highlight is an INSET ROUNDED BLOCK
// ---------------------------------------------------------------------------
// The owner read the 0.0.42 frame as "the selected row's tint spans the whole list
// card edge-to-edge" and asked for the opposite: a block that respects the list's
// horizontal padding and has a corner radius, so the highlight cannot touch the
// card's rounded edges. The geometry is three numbers:
//
//   * `PEER_ROW_HIGHLIGHT_INSET_VP` — the block's left/right inset from the card's
//     inner edge. It IS the list's own content padding (PEER_ROW_LIST_PADDING_VP),
//     because that is the padding the row content uses: the page now writes it as
//     the ROW's own horizontal inset, so "the tint is inset by the content padding"
//     is one constant the selfcheck can compare with itself.
//   * `PEER_LIST_CARD_RADIUS_VP` — the card's own radius (16), which the block's
//     corner has to clear.
//   * `PEER_ROW_HIGHLIGHT_RADIUS_VP` — the block's radius. It is at least the
//     CONCENTRIC radius (`card - inset` = 4, the most a nested rounded box may be
//     and still look nested) and, because a 4 vp radius still reads as a square
//     corner, the shipped value is 12 (the same octave-below-the-cards radius the
//     search field uses). The selfcheck pins both: the bound, and the fact that the
//     block's corner is clear of the card's rounded edge by a stated number of vp.
/** The peer list card's own corner radius (vp). */
export const PEER_LIST_CARD_RADIUS_VP: number = 16;
/** The highlight block's left/right inset from the card's inner edge (vp). */
export const PEER_ROW_HIGHLIGHT_INSET_VP: number = PEER_ROW_LIST_PADDING_VP;
/** The highlight block's corner radius (vp). */
export const PEER_ROW_HIGHLIGHT_RADIUS_VP: number = 12;

/**
 * The SMALLEST radius a block inset by `PEER_ROW_HIGHLIGHT_INSET_VP` inside a
 * `PEER_LIST_CARD_RADIUS_VP` card may have and still read as nested rather than as
 * a square corner poking at the card's arc: the two radii are concentric when
 * `inner = outer - inset`.
 */
export function peerRowHighlightConcentricRadiusVp(): number {
  const concentric: number = PEER_LIST_CARD_RADIUS_VP - PEER_ROW_HIGHLIGHT_INSET_VP;
  return concentric > 0 ? concentric : 0;
}

/**
 * How far (vp) the highlight block's corner is from the card's rounded edge,
 * measured along the corner's own diagonal (the closest approach between the two
 * arcs). Positive = the block cannot touch the card's rounded edge, which is the
 * property the owner asked for; the shipped geometry clears it by ~15 vp.
 */
export function peerRowHighlightCornerClearanceVp(): number {
  const inset: number = PEER_ROW_HIGHLIGHT_INSET_VP;
  const radius: number = PEER_ROW_HIGHLIGHT_RADIUS_VP;
  const cardRadius: number = PEER_LIST_CARD_RADIUS_VP;
  const blockDiagonal: number =
    Math.sqrt((inset + radius) * (inset + radius) * 2) - radius;
  const cardDiagonal: number = Math.sqrt(cardRadius * cardRadius * 2) - cardRadius;
  return blockDiagonal - cardDiagonal;
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
// 0.0.40 (I5) §5 — the GUIDE pane's empty-state block, and §8 the chevron slot
// ---------------------------------------------------------------------------
// §5: the disconnected pane was one small card floating in the middle of a large
// empty area ("更像孤立通知" / "缺少页面归属"). The ASTRA review's minimal
// correction is a real 对端 page: the SAME title and hint line the connected list
// opens with, then a BORDERLESS, width-limited block with the state text and the
// guidance. The block therefore carries no card fill, no stroke and no fixed
// width; it is capped and left-aligned on the page's own text axis, and set 72 vp
// under the title (the review's 64–80 vp) so the empty area reads as the page's
// whitespace rather than as a hole.
/** The guide pane's empty-state block's width cap (vp) — the review's 480. */
export const GUIDE_EMPTY_MAX_WIDTH_VP: number = 480;
/** The gap between the 对端 title block and that empty-state block (vp). */
export const GUIDE_BLOCK_TOP_GAP_VP: number = 72;
/** The guide body's line height (vp) — the review's §5 "行高 22 vp". */
export const GUIDE_BODY_LINE_HEIGHT_FP: number = 22;

// §8: the review could not tell the collapsed sections' chevron from the preview
// text's own punctuation. The preview stays (it is a useful content hint) and the
// disclosure symbol gets a FIXED slot at the row's right edge with a wider gap, so
// the two never read as one run of glyphs. The symbol itself stays the native
// 14 vp ▾/▸ pair the row already used.
/** The collapsed-section chevron's fixed slot at the row's right edge (vp). */
export const SECTION_CHEVRON_SLOT_VP: number = 24;
/** The gap between the preview text and that slot (vp) — the review's §8. */
export const SECTION_HEADER_GAP_VP: number = 12;

// ---------------------------------------------------------------------------
// 0.0.40 (I5) — THE RAIL's connection region: a HEIGHT BUDGET, not a percentage
// ---------------------------------------------------------------------------
// 0.0.39 (I4) capped the connection region at RAIL_CONNECTION_MAX_HEIGHT_PCT
// (60 %) of the rail. The ASTRA review's §2 measured what that cap does: the
// connection card was cut off INSIDE the 详细状态 entry — its subtitle and its
// bottom rounded corner — while ~132 px of slack sat between the card and the
// navigation separator ("到下方导航分隔线之间仍有明显空白"). A percentage cap
// cannot see the slack: it clips at 60 % of the rail whatever the content is and
// whatever room is left.
//
// The fix is the review's own §2 minimal correction ("改为随内容布局 … 不给状态卡
// 一个会在入口中间截断的固定高度"): the region keeps its CONTENT height and gets a
// ceiling that is the space the rail really has — the rail's measured height
// minus what the bottom region (the 1 vp seam + the navigation block) always
// takes. With the ceiling expressed against the rail, the region can only ever
// scroll when the content genuinely does not fit; whenever there is slack, the
// card closes at its own bottom edge. The page hands the rail's measured height
// here (onRootAreaChange's Area.height is in vp) and the block's own Scroll
// carries the short-window case.
//
// The bottom region's height is the SAME numbers the builder lays out (the row
// height, the gap, the two paddings), summed here so the page cannot drift from
// the ceiling it is measured against.
/** The rail's destination row height (vp) — the pointer/touch floor of the rail. */
export const RAIL_NAV_ROW_HEIGHT_VP: number = 48;
/** The gap between the two destination rows (vp). */
export const RAIL_NAV_ROW_GAP_VP: number = 4;
/** The destination block's own top padding (vp) — 8 vp grid. */
export const RAIL_DESTINATIONS_PAD_TOP_VP: number = 8;
/**
 * 0.0.42 (I7) §1: the padding (vp) under the pinned nav — between 设置's row and
 * the rail's bottom edge. This is the owner's "modest bottom padding (~24 vp)"
 * for the pinned arrangement, and it is the block's own bottom padding, so the
 * measured nav → rail-bottom gap IS this number. It retires I5's
 * RAIL_DESTINATIONS_PAD_BOTTOM_VP (12): the bottom padding is no longer an
 * internal 8 vp-grid value but the rail's bottom breathing room, and the ceiling's
 * budget below carries it so the nav can never be the thing a short window clips.
 */
export const RAIL_NAV_BOTTOM_PADDING_VP: number = 24;
/**
 * What the rail's bottom region always takes (vp): the seam, the block's top
 * padding, the two rows, the gap between them and the pinned nav's 24 vp bottom
 * padding. 1 + 8 + 48 + 4 + 48 + 24 = 133 (I6 summed to 121 before the nav was
 * pinned). I5 removed the 导航 caption, so the block is these numbers and nothing
 * else.
 */
export const RAIL_DESTINATIONS_HEIGHT_VP: number =
  RAIL_SEAM_WIDTH_VP + RAIL_DESTINATIONS_PAD_TOP_VP + RAIL_NAV_ROW_HEIGHT_VP +
    RAIL_NAV_ROW_GAP_VP + RAIL_NAV_ROW_HEIGHT_VP + RAIL_NAV_BOTTOM_PADDING_VP;
/**
 * The connection region's floor (vp). The identity block's own head — the 16 vp
 * page padding, the wordmark and the 104 vp disc — needs ~190 vp; 240 keeps the
 * whole head and the card's first rows on screen in a window too short for
 * anything else, and the region's Scroll carries the rest. It is also the
 * compact rail's own width, so no new number enters the file.
 */
export const RAIL_CONNECTION_MIN_VP: number = 240;

/**
 * The connection region's ceiling (vp) for a rail height in vp: everything but
 * the bottom region, floored at RAIL_CONNECTION_MIN_VP so a degenerate height can
 * never produce a negative or collapsing constraint. The floor is what a rail
 * shorter than `RAIL_DESTINATIONS_HEIGHT_VP + RAIL_CONNECTION_MIN_VP` (373 vp with
 * I7's 133 vp bottom region; it was 361 vp) gets; an UNMEASURED height never
 * reaches this function — the page answers
 * '100%' (no ceiling at all) for it, because the failure direction that matters is
 * the card closing at its own bottom edge rather than being cut in the middle of
 * a row.
 */
export function railConnectionMaxHeightVp(railHeightVp: number): number {
  const budget: number = railHeightVp - RAIL_DESTINATIONS_HEIGHT_VP;
  return budget > RAIL_CONNECTION_MIN_VP ? budget : RAIL_CONNECTION_MIN_VP;
}

/**
 * The rail's 详细状态 entry's minimum row height (vp) — the review's §2 "使入口
 * 整行至少 48 vp", i.e. the same pointer floor the destination rows carry.
 */
export const RAIL_ENTRY_MIN_HEIGHT_VP: number = 48;

// ---------------------------------------------------------------------------
// 0.0.41 (I6) §1 / 0.0.42 (I7) §1 — THE RAIL'S VERTICAL RHYTHM: content
// TOP-ANCHORED, nav PINNED TO THE BOTTOM
// ---------------------------------------------------------------------------
// The product owner measured the 0.0.40 frame as ~360 px from the window's top to
// the NetBird wordmark and ~340 px between the status card and the 对端 row, i.e.
// more than half the column empty and the rail's controls spread over the full
// height. The cause was I4's VERTICAL CENTRING: the identity block sat between two
// EQUAL layoutWeight(1) spacers with the destination block pinned to the bottom,
// so all of the rail's slack collected ABOVE the wordmark (and half of the
// remainder under the card). I6 removed both spacers and top-anchored everything —
// which put the whole void under the nav.
//
// The owner has now CHOSEN the other arrangement I6's brief offered, and it is the
// one implemented here:
//   * the connection content stays TOP-ANCHORED — its top gap is unchanged from
//     I6 (RAIL_TOP_GAP_VP = 24 vp: the page padding 16 + the wordmark's own 8);
//   * the 对端 / 设置 block is PINNED TO THE BOTTOM of the rail with a modest
//     24 vp under it (RAIL_NAV_BOTTOM_PADDING_VP);
//   * the empty space therefore ends up BETWEEN the connection content and the nav
//     — one layoutWeight(1) spacer carries it, and there is NO spacer above the
//     content, so vertical centring is NOT re-introduced.
//
// A SHORT window is unchanged in kind and still safe: the connection region keeps
// its absolute ceiling (railConnectionMaxHeightVp = the rail's measured height
// minus the whole bottom region, RAIL_DESTINATIONS_HEIGHT_VP), so the region's own
// Scroll gives way first and the pinned nav is never pushed out or clipped; the
// weighted spacer collapses to 0 before anything else moves. Because the 24 vp now
// belongs to the bottom region, that ceiling's budget includes it — which is
// exactly what "the nav stays visible" costs.
/** The wordmark's own top margin inside the rail's page padding (vp, 8 vp grid). */
export const RAIL_WORDMARK_MARGIN_VP: number = 8;
/**
 * The rail's TOP GAP (vp): from the rail's top edge to the wordmark's top edge —
 * the page padding plus the wordmark's own margin, 16 + 8 = 24. This is the
 * "consistent page padding (~24 vp)" the owner asked for, expressed as the sum
 * the page actually lays out rather than as a number in a comment. I7 keeps it
 * verbatim: pinning the nav to the bottom does not move the content's top.
 */
export const RAIL_TOP_GAP_VP: number = RAIL_PAGE_PADDING_VP + RAIL_WORDMARK_MARGIN_VP;
/**
 * The MINIMUM gap (vp) between the connection card's bottom edge and the first
 * destination row's top edge: the 1 vp region seam plus the destination block's
 * own top padding. 1 + 8 = 9. I7 makes this the FLOOR of that distance rather than
 * the distance itself — the flexible spacer above the seam adds the rail's slack,
 * so the real gap is 9 vp plus whatever the spacer takes and it is exactly 9 vp
 * only in a window too short to have any slack at all.
 */
export const RAIL_DESTINATION_GAP_VP: number =
  RAIL_SEAM_WIDTH_VP + RAIL_DESTINATIONS_PAD_TOP_VP;

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
 * WHICH DETAIL ROWS ARE COPYABLE, AND WHAT THEY COPY.
 *
 * 0.0.43 (I8) §3 introduced the copy mark with a deliberately narrow scope: the
 * domain/hostname and the IP, "no public-key copy in this pass". 0.0.44 (I9) is the
 * pass the owner asked for, and its scope is the whole identifier/address set:
 *
 *   域名            the peer's fqdn (or the row's own value when the page fell back)
 *   NetBird IP      the peer's overlay address
 *   对端端点        the peer's WireGuard endpoint (host:port)
 *   中继通道地址    the relay channel the peer's traffic is carried on
 *   对端网络        the peer's allowed-IP networks
 *   公钥            the peer's public identity key
 *
 * The rows the owner EXCLUDED stay excluded, and they are excluded by KIND rather
 * than by a second list: 状态 / 连接方式 / 最近握手 / 发送 / 接收 / 本机出口 /
 * 承载 / 本机中继会话 / every ICE row answer '' and therefore render NO copy
 * affordance at all. `peerDetailCopyable` is that predicate; the list lives in
 * `peerDetailKindCopyable` (kind → copyable) so the page never keeps one.
 *
 * A row that does not CARRY its value is not copied either: '未知' and the rows'
 * own '未知（核心未上报）' placeholder would otherwise put a placeholder on the
 * clipboard, which is not a copy of anything.
 */
export function peerDetailCopyValue(label: string, value: string): string {
  if (!peerDetailCopyable(label)) {
    return '';
  }
  if (value.length === 0 || value === '未知' || value.indexOf('未知（') === 0) {
    return '';
  }
  return value;
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

// ---------------------------------------------------------------------------
// 0.0.44 (I9) — THE DETAIL ROW'S LEADING GLYPH, ITS FIELD KIND, ITS COPYABILITY
// ---------------------------------------------------------------------------
// The owner's reference frame of the official desktop client draws every detail row
// as: a leading semantic glyph · the label · the value (right-aligned) · a copy mark
// when the value is an identifier or an address. I8 had already drawn the copy mark;
// I9 adds the LEADING glyph to every row of the peer detail pane and widens the copy
// rule to the owner's full identifier/address list.
//
// THE MAPPING IS HERE, NOT IN THE PAGE, for the same reason the section classifier
// is: "which glyph does this row carry, and may its value be copied" is a decision
// the host selfcheck can pin over the WHOLE rendered label set
// (PEER_DETAIL_*_LABELS) without an ArkUI runtime. The page only asks.
//
// A row's FIELD KIND is its semantic identity, and the glyph and the copyability are
// both functions of it, so a row cannot gain one without the other. The shipped
// table (asserted label by label in run-detail-icons.mts):
//
//   label                 field kind        leading glyph   copyable
//   域名                  domain            globe           yes
//   NetBird IP            overlay-ip        pin             yes
//   状态                  state             status dot      no
//   连接方式              mode              cable/link      no
//   最近握手              handshake         clock           no
//   发送 / 接收           tx / rx           up / down       no
//   对端端点              endpoint          node            yes
//   本机出口              egress            plug/exit       no
//   对端网络              networks          node            yes
//   承载                  carrier           cable/link      no
//   中继通道地址          relay-channel     cable/link      yes
//   本机中继会话          relay-session     cable/link      no
//   ICE (the group row)   ice-group         shield          no
//   ICE 状态              ice-state         shield          no
//   ICE 角色              ice-role          branch          no
//   ICE 本端              ice-local         branch          no
//   ICE 对端              ice-remote        branch          no
//   ICE 可达性            ice-reach         shield          no
//   ICE 最近错误          ice-error         shield          no
//   公钥                  key               key             yes
//
// THE VALUE BUDGET IS HERE TOO, because the glyphs are paid for out of the row's
// WIDTH and that arithmetic — not a screenshot — is what says no value was pushed
// into an ellipsis by them. See detailRowValueWidthVp / detailRowValueLines.

/** The leading semantic glyph's box (vp) — the same 16 vp octave as the copy mark. */
export const DETAIL_ROW_ICON_SLOT_VP: number = 16;
/**
 * The gap between the leading glyph and the label (vp). Tighter than
 * DETAIL_ROW_GAP_VP because the glyph reads as part of the label, and because this
 * pair (slot + gap) is the only width the value column pays for the glyph: at the
 * narrowest wide tier the value keeps 108 vp of its former 132 (the copyable rows
 * keep 108 against the copy rows' former 128), which still clears the one-line floor
 * below.
 */
export const DETAIL_ROW_ICON_GAP_VP: number = 4;
/**
 * The gap between the label block and the value, and between the value and the copy
 * mark (vp) — the detail row's own `Row({ space: DETAIL_ROW_GAP_VP })`, unchanged
 * from the row I8 shipped.
 */
export const DETAIL_ROW_GAP_VP: number = 8;
/** The label column (vp) — unchanged since I4, and shared by both branches. */
export const DETAIL_ROW_LABEL_WIDTH_VP: number = 96;
/** The detail card's own horizontal padding (vp) — the row's outer edge. */
export const DETAIL_ROW_CARD_PADDING_VP: number = 16;
/** The detail pane's own horizontal padding around the card (vp). */
export const DETAIL_ROW_PAGE_PADDING_VP: number = RAIL_PAGE_PADDING_VP;
/**
 * The I9 row's rhythm (vp). The official client's detail rows are a 48 vp band with
 * their content centred vertically; the UX standard this project already cites asks
 * for a >= 48 vp row where >= 40 is the must (DESIGN-HARMONY's 触摸热区 row), and the
 * row's own trailing copy target is 40 vp tall — a 33 vp row would let two adjacent
 * rows' touch regions overlap. So the icon-bearing row is a 48 vp band, and the
 * rows WITHOUT one (the settings/关于 pages' read-only rows, unchanged since I4)
 * keep their own content-driven height.
 */
export const DETAIL_ROW_MIN_HEIGHT_VP: number = 48;
/**
 * The detail pane's content-width floor (vp) wherever the shell is mounted: the
 * compact tier's own reservation (320). The narrow (phone/tablet) branch is never
 * narrower either — a 320 vp phone pane is the same arithmetic — so this is the
 * binding case for the row's value budget.
 */
export const DETAIL_ROW_PANE_MIN_WIDTH_VP: number = PEER_LIST_CONTENT_MIN_COMPACT_VP;
/** The characters the value column must fit on ONE line: the overlay IPv4 it leads with. */
export const DETAIL_ROW_VALUE_CHARS: number = 14;
/**
 * The width budget of one value character at TYPE_BODY_FP — 0.5 em, the same
 * proportional-Latin rule PEER_ROW_NAME_CHAR_VP states at 14 vp.
 */
export const DETAIL_ROW_VALUE_CHAR_VP: number = TYPE_BODY_FP / 2;
/** The floor the value column must clear: that IPv4 (14 × 7.5 = 105 vp) on one line. */
export const DETAIL_ROW_VALUE_MIN_WIDTH_VP: number =
  DETAIL_ROW_VALUE_CHARS * DETAIL_ROW_VALUE_CHAR_VP;
/** The longest value the pane renders: the 64-character public key, in mono. */
export const DETAIL_ROW_LONGEST_VALUE_CHARS: number = 64;
/** A monospace glyph's advance (vp) at TYPE_BODY_FP — the 0.6 em rule. */
export const DETAIL_ROW_MONO_CHAR_VP: number = TYPE_BODY_FP * 0.6;
/** The line cap the peer detail's rows are mounted with (DetailRow's own default). */
export const DETAIL_ROW_VALUE_MAX_LINES: number = 8;

// ---- the field kinds --------------------------------------------------------
export const DETAIL_KIND_DOMAIN: string = 'domain';
export const DETAIL_KIND_OVERLAY_IP: string = 'overlay-ip';
export const DETAIL_KIND_STATE: string = 'state';
export const DETAIL_KIND_MODE: string = 'mode';
export const DETAIL_KIND_HANDSHAKE: string = 'handshake';
export const DETAIL_KIND_TX: string = 'tx';
export const DETAIL_KIND_RX: string = 'rx';
export const DETAIL_KIND_ENDPOINT: string = 'endpoint';
export const DETAIL_KIND_EGRESS: string = 'egress';
export const DETAIL_KIND_NETWORKS: string = 'networks';
export const DETAIL_KIND_CARRIER: string = 'carrier';
export const DETAIL_KIND_RELAY_CHANNEL: string = 'relay-channel';
export const DETAIL_KIND_RELAY_SESSION: string = 'relay-session';
export const DETAIL_KIND_ICE_GROUP: string = 'ice-group';
export const DETAIL_KIND_ICE_STATE: string = 'ice-state';
export const DETAIL_KIND_ICE_ROLE: string = 'ice-role';
export const DETAIL_KIND_ICE_LOCAL: string = 'ice-local';
export const DETAIL_KIND_ICE_REMOTE: string = 'ice-remote';
export const DETAIL_KIND_ICE_REACH: string = 'ice-reach';
export const DETAIL_KIND_ICE_ERROR: string = 'ice-error';
export const DETAIL_KIND_KEY: string = 'key';
/** A label the classifier does not know: no glyph, no copy — nothing is invented. */
export const DETAIL_KIND_NONE: string = '';

// ---- the glyph kinds --------------------------------------------------------
/** location/pin — NetBird IP. */
export const DETAIL_ICON_PIN: string = 'pin';
/** globe — 域名 / hostname. */
export const DETAIL_ICON_GLOBE: string = 'globe';
/** node/endpoint — 对端端点, 对端网络. */
export const DETAIL_ICON_NODE: string = 'node';
/** cable/link — 承载, 中继通道地址, 连接方式, 本机中继会话. */
export const DETAIL_ICON_LINK: string = 'link';
/** key — 公钥. */
export const DETAIL_ICON_KEY: string = 'key';
/** clock — 最近握手. */
export const DETAIL_ICON_CLOCK: string = 'clock';
/** status dot — 状态. */
export const DETAIL_ICON_STATUS: string = 'status';
/** plug/exit — 本机出口. */
export const DETAIL_ICON_EXIT: string = 'exit';
/** arrow up / arrow down — 发送 / 接收. */
export const DETAIL_ICON_UP: string = 'up';
export const DETAIL_ICON_DOWN: string = 'down';
/** shield — the ICE verdict rows (the group row, 状态, 可达性, 最近错误). */
export const DETAIL_ICON_SHIELD: string = 'shield';
/** branch — the ICE candidate-path rows (角色, 本端, 对端). */
export const DETAIL_ICON_BRANCH: string = 'branch';
/** The I8 copy mark (two overlapping rounded sheets) — the row's TRAILING glyph. */
export const DETAIL_ICON_COPY: string = 'copy';
/**
 * Every glyph the one icon component knows how to draw. The selfcheck walks this
 * list against the page's switch, so a kind added here but not drawn there is a
 * failing test rather than a blank 16 vp box on the device.
 */
export const DETAIL_ICON_KINDS: string[] = [
  DETAIL_ICON_PIN, DETAIL_ICON_GLOBE, DETAIL_ICON_NODE, DETAIL_ICON_LINK,
  DETAIL_ICON_KEY, DETAIL_ICON_CLOCK, DETAIL_ICON_STATUS, DETAIL_ICON_EXIT,
  DETAIL_ICON_UP, DETAIL_ICON_DOWN, DETAIL_ICON_SHIELD, DETAIL_ICON_BRANCH,
  DETAIL_ICON_COPY
];

/**
 * The row's FIELD KIND (its semantic identity). Every label the pane renders has
 * one; an unknown label is DETAIL_KIND_NONE, i.e. no glyph and no copy.
 */
export function peerDetailFieldKind(label: string): string {
  if (label === '域名') {
    return DETAIL_KIND_DOMAIN;
  }
  if (label === 'NetBird IP') {
    return DETAIL_KIND_OVERLAY_IP;
  }
  if (label === '状态') {
    return DETAIL_KIND_STATE;
  }
  if (label === '连接方式') {
    return DETAIL_KIND_MODE;
  }
  if (label === '最近握手') {
    return DETAIL_KIND_HANDSHAKE;
  }
  if (label === '发送') {
    return DETAIL_KIND_TX;
  }
  if (label === '接收') {
    return DETAIL_KIND_RX;
  }
  if (label === '对端端点') {
    return DETAIL_KIND_ENDPOINT;
  }
  if (label === '本机出口') {
    return DETAIL_KIND_EGRESS;
  }
  if (label === '对端网络') {
    return DETAIL_KIND_NETWORKS;
  }
  if (label === '承载') {
    return DETAIL_KIND_CARRIER;
  }
  if (label === '中继通道地址') {
    return DETAIL_KIND_RELAY_CHANNEL;
  }
  if (label === '本机中继会话') {
    return DETAIL_KIND_RELAY_SESSION;
  }
  if (label === 'ICE') {
    return DETAIL_KIND_ICE_GROUP;
  }
  if (label === 'ICE 状态') {
    return DETAIL_KIND_ICE_STATE;
  }
  if (label === 'ICE 角色') {
    return DETAIL_KIND_ICE_ROLE;
  }
  if (label === 'ICE 本端') {
    return DETAIL_KIND_ICE_LOCAL;
  }
  if (label === 'ICE 对端') {
    return DETAIL_KIND_ICE_REMOTE;
  }
  if (label === 'ICE 可达性') {
    return DETAIL_KIND_ICE_REACH;
  }
  if (label === 'ICE 最近错误') {
    return DETAIL_KIND_ICE_ERROR;
  }
  if (label === '公钥') {
    return DETAIL_KIND_KEY;
  }
  return DETAIL_KIND_NONE;
}

/**
 * Field kind → the leading glyph it draws. Every kind has one, so a rendered row
 * whose label classifies cannot be glyph-less; an unknown kind draws nothing.
 */
export function peerDetailKindIcon(kind: string): string {
  if (kind === DETAIL_KIND_DOMAIN) {
    return DETAIL_ICON_GLOBE;
  }
  if (kind === DETAIL_KIND_OVERLAY_IP) {
    return DETAIL_ICON_PIN;
  }
  if (kind === DETAIL_KIND_STATE) {
    return DETAIL_ICON_STATUS;
  }
  if (kind === DETAIL_KIND_MODE) {
    return DETAIL_ICON_LINK;
  }
  if (kind === DETAIL_KIND_HANDSHAKE) {
    return DETAIL_ICON_CLOCK;
  }
  if (kind === DETAIL_KIND_TX) {
    return DETAIL_ICON_UP;
  }
  if (kind === DETAIL_KIND_RX) {
    return DETAIL_ICON_DOWN;
  }
  if (kind === DETAIL_KIND_ENDPOINT || kind === DETAIL_KIND_NETWORKS) {
    return DETAIL_ICON_NODE;
  }
  if (kind === DETAIL_KIND_EGRESS) {
    return DETAIL_ICON_EXIT;
  }
  if (kind === DETAIL_KIND_CARRIER || kind === DETAIL_KIND_RELAY_CHANNEL ||
    kind === DETAIL_KIND_RELAY_SESSION) {
    return DETAIL_ICON_LINK;
  }
  if (kind === DETAIL_KIND_ICE_GROUP || kind === DETAIL_KIND_ICE_STATE ||
    kind === DETAIL_KIND_ICE_REACH || kind === DETAIL_KIND_ICE_ERROR) {
    return DETAIL_ICON_SHIELD;
  }
  if (kind === DETAIL_KIND_ICE_ROLE || kind === DETAIL_KIND_ICE_LOCAL ||
    kind === DETAIL_KIND_ICE_REMOTE) {
    return DETAIL_ICON_BRANCH;
  }
  if (kind === DETAIL_KIND_KEY) {
    return DETAIL_ICON_KEY;
  }
  return DETAIL_KIND_NONE;
}

/** The row's label → the leading glyph it draws. */
export function peerDetailIconKind(label: string): string {
  return peerDetailKindIcon(peerDetailFieldKind(label));
}

/**
 * Field kind → may this row's value be copied? THE OWNER'S LIST, and nothing else:
 * the six identifier/address kinds. The state, the mode, the handshake time, the
 * traffic counters, the local egress, the bearer label, the local relay session and
 * every ICE row are FALSE.
 */
export function peerDetailKindCopyable(kind: string): boolean {
  return kind === DETAIL_KIND_DOMAIN || kind === DETAIL_KIND_OVERLAY_IP ||
    kind === DETAIL_KIND_ENDPOINT || kind === DETAIL_KIND_RELAY_CHANNEL ||
    kind === DETAIL_KIND_NETWORKS || kind === DETAIL_KIND_KEY;
}

/** The row's label → may its value be copied? */
export function peerDetailCopyable(label: string): boolean {
  return peerDetailKindCopyable(peerDetailFieldKind(label));
}

/**
 * The width (vp) the detail pane's CONTENT has at a window width: the pane's width
 * (window − rail) minus the list column Navigation carves out of it — the same two
 * functions the shell itself is built from, so the row budget cannot drift from the
 * layout. Never negative.
 */
export function detailPaneContentWidthVp(windowVp: number): number {
  const pane: number = splitPaneWidthVp(windowVp);
  const navBar: number = peerListNavBarWidthVp(pane, windowVp);
  const content: number = pane - navBar;
  return content > 0 ? content : 0;
}

/**
 * The width (vp) a detail row's VALUE column has in a pane of this width: the pane,
 * less the page's padding, the card's padding, the leading glyph and its gap, the
 * label column and its gap, and — for a copyable row — the copy mark and its gap.
 * This is the number the ellipsis argument rests on; never negative.
 *
 * 0.0.45 (I10) — the copy mark's gap is COPY_ICON_GAP_VP (5), not the row's own
 * DETAIL_ROW_GAP_VP (8): the value is End-aligned inside the column, so its last
 * glyph sits ON the column's right edge and the mark's 5 vp is what the eye reads.
 * The value column therefore GREW by 3 vp (108 → 111 at the narrowest wide tier),
 * which is why the floor's slack is now 6 vp and not 3. The label↔value gap is the
 * row's own DETAIL_ROW_GAP_VP and is unchanged.
 */
export function detailRowValueWidthVp(paneWidthVp: number, copyable: boolean): number {
  let width: number = paneWidthVp - 2 * DETAIL_ROW_PAGE_PADDING_VP -
    2 * DETAIL_ROW_CARD_PADDING_VP;
  width -= DETAIL_ROW_ICON_SLOT_VP + DETAIL_ROW_ICON_GAP_VP;
  width -= DETAIL_ROW_LABEL_WIDTH_VP + DETAIL_ROW_GAP_VP;
  if (copyable) {
    width -= COPY_ICON_GAP_VP + PEER_ROW_COPY_SLOT_VP;
  }
  return width > 0 ? width : 0;
}

/**
 * The lines a value of `chars` characters at `charVp` per character wraps to in a
 * column `valueWidthVp` wide — the estimate `<= DETAIL_ROW_VALUE_MAX_LINES` is
 * exactly "the row's value is not ellipsised". A nonsense argument answers 0 (the
 * empty value), never NaN and never a divide by zero.
 */
export function detailRowValueLines(chars: number, charVp: number,
  valueWidthVp: number): number {
  if (!(chars > 0) || !(charVp > 0) || !(valueWidthVp > 0)) {
    return 0;
  }
  return Math.ceil((chars * charVp) / valueWidthVp);
}

/**
 * 0.0.46 (I11) — THE LINES THE LONGEST VALUE WRAPS TO AT THE ROW CAP: the 64-char
 * monospace public key in a pane's value column, with the mono advance paid at
 * TYPE_SCALE_MAX_ROW. `<= DETAIL_ROW_VALUE_MAX_LINES` is exactly "the row's value is
 * never ellipsised", and it is the second of the two statements the cap of 1.3 was
 * derived from (7 lines of the 8 allowed at the narrowest 320 vp pane; 11 at 1.75).
 */
export function detailRowLongestValueLinesAtCap(paneWidthVp: number,
  copyable: boolean): number {
  return detailRowValueLines(DETAIL_ROW_LONGEST_VALUE_CHARS,
    DETAIL_ROW_MONO_CHAR_VP * TYPE_SCALE_MAX_ROW,
    detailRowValueWidthVp(paneWidthVp, copyable));
}
