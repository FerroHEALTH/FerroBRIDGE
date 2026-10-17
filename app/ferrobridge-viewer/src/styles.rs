// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's design vocabulary, as the class strings the shell paints
//! with.
//!
//! A class names a role of `style/tailwind.css` (`surface`, `raised`, `ink`,
//! `muted`, `accent`, `line`) and a step of Tailwind's 4 px spacing scale,
//! never a palette entry, so the palette is measured once for WCAG 2.2 AA and
//! a theme is one block of that stylesheet. The views write plain semantic
//! HTML, which the stylesheet's base layer draws.

/// The page ground and the text on it.
pub const PAGE: &str = "flex min-h-screen flex-col bg-surface text-ink";

/// The link that skips the chrome to the page, shown once it has the focus.
pub const SKIP: &str = "sr-only rounded-control bg-raised px-3 py-2 focus:not-sr-only \
                        focus:absolute focus:top-2 focus:left-2 focus:z-50";

/// The bar across the top of every signed-in page, which stays in view from
/// `md` up.
pub const TOPBAR: &str = "border-b border-line bg-raised md:sticky md:top-0 md:z-30";

/// What the top bar holds; from `md` up it is one row of the stylesheet's
/// `--bar` height, which the navigation and the scroll padding read.
pub const TOPBAR_ROW: &str = "flex min-h-14 flex-wrap items-center gap-x-3 gap-y-2 px-4 py-2 \
                              md:h-[calc(var(--bar)-1px)] md:flex-nowrap md:px-6";

/// The far end of the top bar.
pub const TOPBAR_END: &str = "ml-auto flex flex-wrap items-center justify-end gap-2 md:flex-nowrap";

/// The link the mark and the product name make together.
pub const LOCKUP: &str = "flex items-center gap-2 text-ink no-underline hover:text-ink";

/// The product name beside the mark.
pub const LOCKUP_NAME: &str = "text-base font-semibold whitespace-nowrap";

/// The word of the product name the accent draws.
pub const LOCKUP_ACCENT: &str = "text-accent";

/// The brand mark, which the stylesheet draws for the theme.
pub const MARK: &str = "brand-mark";

/// The navigation and the page beside it.
pub const BODY: &str = "flex flex-1";

/// The navigation: a drawer below `md`, a column beside the page from it.
pub const SIDEBAR: &str = "drawer shrink-0 bg-raised p-3 text-ink";

/// The heading of a group of navigation entries.
pub const NAV_GROUP: &str = "px-3 pt-4 pb-1 text-xs font-semibold tracking-wide text-muted \
                             uppercase first:pt-1";

/// A list of navigation entries.
pub const NAV_LIST: &str = "m-0 flex list-none flex-col gap-0.5 p-0";

/// A navigation entry: while it is the page, tinted with the accent, set in
/// bold and marked by a bar at its start, so colour is never the only mark.
pub const NAV_ENTRY: &str = "state-change flex items-center gap-3 rounded-control px-3 py-2 \
                             font-medium text-muted no-underline hover:bg-inset hover:text-ink \
                             aria-[current=page]:bg-accent-tint aria-[current=page]:text-accent \
                             aria-[current=page]:font-semibold \
                             aria-[current=page]:shadow-[inset_3px_0_0_var(--accent)]";

/// The page itself, which never scrolls sideways.
pub const MAIN: &str = "min-w-0 flex-1 px-4 py-6 md:px-8";

/// The column the page reads in.
pub const COLUMN: &str = "mx-auto max-w-7xl";

/// The one line at the foot of every page.
pub const FOOTER: &str = "border-t border-line bg-raised px-4 py-3 text-xs text-muted md:px-6";

/// A page that centres one card: the sign-in, a page the console does not
/// serve, a fault.
pub const CENTRED: &str = "flex flex-1 flex-col items-center justify-center px-4 py-12 \
                           text-center";

/// A card: a raised surface with a hairline edge and a soft shadow.
pub const CARD: &str = "w-full max-w-md rounded-card border border-line bg-raised p-6 \
                        shadow-card";

/// The one action a screen is for, as a link.
pub const BUTTON_PRIMARY: &str = "state-change inline-flex items-center justify-center gap-2 \
                                  rounded-control px-3.5 py-2 text-sm font-medium no-underline \
                                  bg-accent text-accent-fg hover:bg-accent-hover \
                                  hover:text-accent-fg";

/// An action beside the primary one.
pub const BUTTON_SECONDARY: &str = "state-change inline-flex items-center justify-center gap-2 \
                                    rounded-control px-3.5 py-2 text-sm font-medium \
                                    no-underline border border-line-strong bg-raised text-ink \
                                    hover:bg-inset hover:text-ink";

/// An action that does not stand out: the drawer's close.
pub const BUTTON_GHOST: &str = "state-change inline-flex items-center justify-center gap-2 \
                                rounded-control px-3.5 py-2 text-sm font-medium no-underline \
                                bg-transparent text-muted hover:bg-inset hover:text-ink";

/// The theme switch: three buttons in one segmented control.
pub const SEGMENTED: &str = "inline-flex items-center gap-0.5 rounded-control border \
                             border-line bg-surface p-0.5";

/// One button of the theme switch: while it is the theme in force, raised,
/// edged at 3:1 against the switch's ground, and marked pressed.
pub const SEGMENT: &str = "state-change inline-flex items-center gap-1.5 rounded-[0.25rem] \
                           border border-transparent bg-transparent px-2 py-1 text-xs \
                           font-medium text-muted hover:bg-inset hover:text-ink \
                           aria-pressed:bg-raised aria-pressed:border-control \
                           aria-pressed:text-ink aria-pressed:shadow-card";

/// The operator's menu button in the top bar.
pub const MENU_BUTTON: &str = "state-change inline-flex items-center gap-2 rounded-control \
                               border border-line bg-raised px-2.5 py-1.5 text-sm font-medium \
                               text-ink hover:bg-inset";

/// The button that opens the navigation drawer, below `md` alone.
pub const DRAWER_BUTTON: &str = "state-change inline-flex items-center gap-2 rounded-control \
                                 border border-line bg-raised px-2.5 py-1.5 text-sm \
                                 font-medium text-ink hover:bg-inset md:hidden";

/// The operator's menu: a card that opens under the top bar.
pub const MENU: &str = "menu rounded-card border border-line bg-raised p-3 text-ink shadow-card";

/// A status tone: the ground, the text and the dot of a pill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// It works.
    Ok,
    /// It may not work.
    Warn,
    /// It does not work.
    Danger,
    /// Nothing is known yet.
    Neutral,
}

impl Tone {
    /// The classes of a status pill in this tone.
    #[must_use]
    pub const fn pill(self) -> &'static str {
        match self {
            Self::Ok => "pill tone-ok",
            Self::Warn => "pill tone-warn",
            Self::Danger => "pill tone-danger",
            Self::Neutral => "pill tone-neutral",
        }
    }
}
