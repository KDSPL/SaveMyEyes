// macOS screen overlay using borderless transparent NSWindows.
//
// Strategy:
//   • One NSWindow per screen (NSScreen.screens)
//   • Window level = .screenSaver (above everything)
//   • Ignores mouse events (click-through)
//   • Semi-transparent black background with adjustable alpha
//   • Capture-safe: setSharingType(.none) hides from screenshots & recordings
//
// Multi-monitor:
//   Each overlay window is mapped to a screen index.
//   Per-monitor opacity is stored in the config.

use std::collections::HashMap;
use std::sync::Mutex;

use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSScreen, NSScreenSaverWindowLevel, NSWindow,
    NSWindowCollectionBehavior, NSWindowStyleMask, NSWindowSharingType,
};
use objc2_foundation::{NSRect, NSUInteger};

struct OverlayWindow {
    window: Retained<NSWindow>,
    #[allow(dead_code)]
    screen_index: u32,
}

// Safety: All overlay access is dispatched to the main thread via GCD.
unsafe impl Send for OverlayWindow {}
unsafe impl Sync for OverlayWindow {}

static OVERLAY_WINDOWS: Mutex<Vec<OverlayWindow>> = Mutex::new(Vec::new());

/// Show overlay windows on screens.
/// When multi_monitor is false, only the primary (main) screen is dimmed.
/// When multi_monitor is true, all screens are dimmed (each with its own opacity).
pub fn show(
    mtm: MainThreadMarker,
    global_opacity: f32,
    multi_monitor: bool,
    per_display: &HashMap<String, f32>,
) {
    hide();

    let screens = NSScreen::screens(mtm);
    let count = screens.count() as usize;
    let names = screen_names(mtm);

    let mut windows = OVERLAY_WINDOWS.lock().unwrap();

    for i in 0..count {
        // When multi-monitor is off, only dim the primary screen (index 0)
        if !multi_monitor && i > 0 {
            continue;
        }

        let screen = screens.objectAtIndex(i as NSUInteger);
        let frame: NSRect = screen.frame();

        let opacity = if multi_monitor {
            names
                .get(i)
                .and_then(|n| per_display.get(n))
                .copied()
                .unwrap_or(global_opacity)
        } else {
            global_opacity
        };

        let window = create_overlay_window(mtm, frame, opacity);

        // Configure behavior — appear on all Spaces, stationary, hidden from Exposé
        // FullScreenAuxiliary prevents flashing during space transitions
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );

        // Capture-safe: hide from screenshots and screen recordings
        window.setSharingType(NSWindowSharingType::None);

        window.orderFrontRegardless();

        windows.push(OverlayWindow {
            window,
            screen_index: i as u32,
        });
    }
}

/// Hide and destroy all overlay windows.
pub fn hide() {
    let mut windows = OVERLAY_WINDOWS.lock().unwrap();
    for ow in windows.drain(..) {
        ow.window.orderOut(None);
    }
}

/// Set opacity on all overlay windows (single-monitor mode).
pub fn set_opacity(opacity: f32) {
    let windows = OVERLAY_WINDOWS.lock().unwrap();
    for ow in windows.iter() {
        ow.window.setAlphaValue(opacity as f64);
    }
}

/// Update opacity on existing overlay windows without recreating them.
/// Returns false if no overlays exist (caller should use show() instead).
pub fn update_opacity(
    mtm: MainThreadMarker,
    global_opacity: f32,
    multi_monitor: bool,
    per_display: &HashMap<String, f32>,
) -> bool {
    let windows = OVERLAY_WINDOWS.lock().unwrap();
    if windows.is_empty() {
        return false;
    }
    let names = screen_names(mtm);
    for ow in windows.iter() {
        let opacity = if multi_monitor {
            names
                .get(ow.screen_index as usize)
                .and_then(|n| per_display.get(n))
                .copied()
                .unwrap_or(global_opacity)
        } else {
            global_opacity
        };
        ow.window.setAlphaValue(opacity as f64);
    }
    true
}

/// Set opacity on a specific monitor's overlay window.
#[allow(dead_code)]
pub fn set_monitor_opacity(monitor_index: u32, opacity: f32) {
    let windows = OVERLAY_WINDOWS.lock().unwrap();
    for ow in windows.iter() {
        if ow.screen_index == monitor_index {
            ow.window.setAlphaValue(opacity as f64);
        }
    }
}

/// Check if overlays are visible.
pub fn is_visible() -> bool {
    !OVERLAY_WINDOWS.lock().unwrap().is_empty()
}

/// Get the screen index that contains the given point (mouse cursor).
#[allow(dead_code)]
pub fn screen_index_at_point(mtm: MainThreadMarker, x: f64, y: f64) -> u32 {
    let screens = NSScreen::screens(mtm);
    let count = screens.count() as usize;
    for i in 0..count {
        let screen = screens.objectAtIndex(i as NSUInteger);
        let frame: NSRect = screen.frame();
        if x >= frame.origin.x
            && x < frame.origin.x + frame.size.width
            && y >= frame.origin.y
            && y < frame.origin.y + frame.size.height
        {
            return i as u32;
        }
    }
    0
}

/// Get total number of screens.
#[allow(dead_code)]
pub fn screen_count(mtm: MainThreadMarker) -> u32 {
    NSScreen::screens(mtm).count() as u32
}

/// Get display names for all connected screens.
/// Returns a Vec of human-readable display names (e.g. "Built-in Retina Display", "DELL U2723QE").
pub fn screen_names(mtm: MainThreadMarker) -> Vec<String> {
    let screens = NSScreen::screens(mtm);
    let count = screens.count() as usize;
    let mut names = Vec::with_capacity(count);
    for i in 0..count {
        let screen = screens.objectAtIndex(i as NSUInteger);
        let name = screen.localizedName().to_string();
        // Deduplicate: if same name already exists, append index
        let final_name = if names.contains(&name) {
            format!("{} ({})", name, i + 1)
        } else {
            name
        };
        names.push(final_name);
    }
    names
}

/// Refresh overlay windows (e.g. after screen config changes).
/// Re-creates overlays with current config.
#[allow(dead_code)]
pub fn refresh(mtm: MainThreadMarker, global_opacity: f32, multi_monitor: bool, per_display: &HashMap<String, f32>) {
    if is_visible() {
        show(mtm, global_opacity, multi_monitor, per_display);
    }
}

// ── Internal ────────────────────────────────────────────────────────────────

fn create_overlay_window(mtm: MainThreadMarker, frame: NSRect, opacity: f32) -> Retained<NSWindow> {
    let style = NSWindowStyleMask::Borderless;

    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            mtm.alloc::<NSWindow>(),
            frame,
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };

    // Make it a transparent overlay
    window.setOpaque(false);
    let black = NSColor::colorWithRed_green_blue_alpha(0.0, 0.0, 0.0, 1.0);
    window.setBackgroundColor(Some(&black));
    window.setAlphaValue(opacity as f64);

    // Click-through: ignore all mouse events
    window.setIgnoresMouseEvents(true);

    // Above everything — use a very high level so no other app window
    // can appear over the dimming overlay.
    window.setLevel(NSScreenSaverWindowLevel + 1000);

    // Don't show in dock or Exposé
    window.setHidesOnDeactivate(false);

    window
}
