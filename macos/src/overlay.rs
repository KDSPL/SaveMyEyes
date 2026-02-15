// macOS screen overlay using borderless transparent NSWindows.
//
// Strategy:
//   • One NSWindow per screen (NSScreen.screens)
//   • Window level = .screenSaver (above everything)
//   • Ignores mouse events (click-through)
//   • Semi-transparent black background with adjustable alpha
//
// Multi-monitor:
//   Each overlay window is mapped to a screen index.
//   Per-monitor opacity is stored in the config.

use std::collections::HashMap;
use std::sync::Mutex;

use objc2::rc::Retained;
use objc2::{msg_send, MainThreadMarker};
use objc2_app_kit::{
    NSColor, NSScreen, NSWindow, NSWindowCollectionBehavior, NSWindowLevel,
    NSWindowStyleMask,
};
use objc2_foundation::NSRect;

struct OverlayWindow {
    window: Retained<NSWindow>,
    screen_index: u32,
}

static OVERLAY_WINDOWS: Mutex<Vec<OverlayWindow>> = Mutex::new(Vec::new());

/// Show overlay windows on all screens.
pub fn show(
    mtm: MainThreadMarker,
    global_opacity: f32,
    multi_monitor: bool,
    per_monitor: &HashMap<u32, f32>,
) {
    hide();

    let screens = NSScreen::screens(mtm);
    let count = screens.len();

    let mut windows = OVERLAY_WINDOWS.lock().unwrap();

    for i in 0..count {
        let screen = &screens[i];
        let frame: NSRect = screen.frame();

        let opacity = if multi_monitor {
            per_monitor.get(&(i as u32)).copied().unwrap_or(global_opacity)
        } else {
            global_opacity
        };

        let window = create_overlay_window(frame, opacity);

        // Configure behavior
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );

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

/// Set opacity on a specific monitor's overlay window.
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
pub fn screen_index_at_point(mtm: MainThreadMarker, x: f64, y: f64) -> u32 {
    let screens = NSScreen::screens(mtm);
    for (i, screen) in screens.iter().enumerate() {
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
pub fn screen_count(mtm: MainThreadMarker) -> u32 {
    NSScreen::screens(mtm).len() as u32
}

// ── Internal ────────────────────────────────────────────────────────────────

fn create_overlay_window(frame: NSRect, opacity: f32) -> Retained<NSWindow> {
    let style = NSWindowStyleMask::Borderless;

    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(),
            frame,
            style,
            objc2_app_kit::NSBackingStoreType::NSBackingStoreBuffered,
            false,
        )
    };

    // Make it a transparent overlay
    window.setOpaque(false);
    let black = unsafe { NSColor::colorWithRed_green_blue_alpha(0.0, 0.0, 0.0, 1.0) };
    window.setBackgroundColor(Some(&black));
    window.setAlphaValue(opacity as f64);

    // Click-through: ignore all mouse events
    window.setIgnoresMouseEvents(true);

    // Above everything
    window.setLevel(NSWindowLevel(
        objc2_app_kit::NSScreenSaverWindowLevel as isize + 1,
    ));

    // Don't show in dock or Exposé
    window.setHidesOnDeactivate(false);

    window
}
