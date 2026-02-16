#![allow(dead_code)]
// Theme constants for the macOS Settings window.
// Mirrors the shadcn dark theme from the Windows version but uses
// Core Graphics / NSColor conventions (RGBA floats 0.0–1.0).

/// Background: #030711
pub const CLR_BG: (f64, f64, f64) = (0.012, 0.027, 0.067);

/// Foreground / primary text: #F8FAFC
pub const CLR_FG: (f64, f64, f64) = (0.973, 0.980, 0.988);

/// Secondary / card bg / borders: #1E293B
pub const CLR_SECONDARY: (f64, f64, f64) = (0.118, 0.161, 0.231);

/// Muted foreground (labels, descriptions): #94A3B8
pub const CLR_MUTED: (f64, f64, f64) = (0.580, 0.639, 0.722);

/// Brand purple: #7C3AED
pub const CLR_BRAND: (f64, f64, f64) = (0.486, 0.227, 0.929);

/// Slider track inactive: #334155
pub const CLR_TRACK: (f64, f64, f64) = (0.2, 0.255, 0.333);

/// Toggle off: same as secondary
pub const CLR_TOGGLE_OFF: (f64, f64, f64) = CLR_SECONDARY;

/// Toggle knob: white
pub const CLR_TOGGLE_KNOB: (f64, f64, f64) = (1.0, 1.0, 1.0);

// ── Dimensions ──────────────────────────────────────────────────────────────

pub const WINDOW_W: f64 = 400.0;
pub const WINDOW_H: f64 = 580.0;
pub const PADDING: f64 = 24.0;
pub const CONTENT_W: f64 = WINDOW_W - 2.0 * PADDING;
pub const CARD_RADIUS: f64 = 8.0;
pub const GAP: f64 = 12.0;
pub const TAB_H: f64 = 36.0;

pub const FONT_SIZE_TITLE: f64 = 18.0;
pub const FONT_SIZE_NORMAL: f64 = 13.0;
pub const FONT_SIZE_SMALL: f64 = 11.0;
pub const FONT_SIZE_XS: f64 = 10.0;

pub const SLIDER_H: f64 = 6.0;
pub const SLIDER_THUMB_R: f64 = 8.0;
pub const TOGGLE_W: f64 = 44.0;
pub const TOGGLE_H: f64 = 24.0;
