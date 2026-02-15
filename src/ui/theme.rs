use windows::Win32::Foundation::COLORREF;

// ── Color palette matching the shadcn dark theme exactly ─────────────────────

/// Background: hsl(222.2, 84%, 4.9%) → #030711
pub const CLR_BACKGROUND: COLORREF = COLORREF(0x00110703);

/// Foreground / primary text: hsl(210, 40%, 98%) → #F8FAFC
pub const CLR_FOREGROUND: COLORREF = COLORREF(0x00FCFAF8);

/// Secondary / card borders / muted bg: hsl(217.2, 32.6%, 17.5%) → #1E293B
pub const CLR_SECONDARY: COLORREF = COLORREF(0x003B291E);

/// Muted foreground (descriptions, labels): hsl(215, 20.2%, 65.1%) → #94A3B8
pub const CLR_MUTED_FG: COLORREF = COLORREF(0x00B8A394);

/// Brand purple: hsl(262, 83%, 58%) → #7C3AED
pub const CLR_BRAND: COLORREF = COLORREF(0x00ED3A7C);

/// Border color (same as secondary)
pub const CLR_BORDER: COLORREF = COLORREF(0x003B291E);

/// Input/toggle background (same as secondary)
pub const CLR_INPUT: COLORREF = COLORREF(0x003B291E);

// ── Dimensions ───────────────────────────────────────────────────────────────

/// Main window client area dimensions
pub const WINDOW_WIDTH: i32 = 400;
pub const WINDOW_HEIGHT: i32 = 580;

/// Padding inside the window
pub const PADDING: i32 = 24;

/// Content width (WINDOW_WIDTH - 2 * PADDING)
pub const CONTENT_WIDTH: i32 = WINDOW_WIDTH - 2 * PADDING;

/// Card border radius
pub const CARD_RADIUS: i32 = 8;

/// Tab bar height
pub const TAB_HEIGHT: i32 = 36;

/// Gap between sections
pub const GAP: i32 = 12;

// ── Font sizes (in logical units, negative for character height) ─────────────

pub const FONT_SIZE_TITLE: i32 = -18; // ~1.25rem → h1 title

pub const FONT_SIZE_SMALL: i32 = -12; // 0.875rem
pub const FONT_SIZE_XS: i32 = -11; // 0.75rem
pub const FONT_SIZE_XXS: i32 = -10; // 0.7rem

/// Font family name
pub const FONT_NAME: &str = "Segoe UI";
pub const FONT_MONO_NAME: &str = "Consolas";
