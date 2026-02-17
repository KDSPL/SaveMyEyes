// macOS screen dimming using Core Graphics gamma tables.
//
// Strategy:
//   • Manipulate each display's gamma transfer formula via
//     CGSetDisplayTransferByFormula to reduce luminance.
//   • Works everywhere — including full-screen apps, screen saver,
//     and login window — because gamma is applied at the GPU output
//     stage, not via window layering.
//   • Invisible to screenshots and screen recordings (capture-safe)
//     because gamma changes are applied after framebuffer composition.
//
// Multi-monitor:
//   Each display is identified by CGDirectDisplayID and mapped to
//   the user-facing NSScreen.localizedName(). Per-display opacity is
//   stored in config keyed by display name.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;
use objc2_foundation::NSUInteger;

// ── Core Graphics FFI ───────────────────────────────────────────────────────

type CGDirectDisplayID = u32;
type CGGammaValue = f32;
type CGError = i32;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGMainDisplayID() -> CGDirectDisplayID;
    fn CGGetActiveDisplayList(
        max_displays: u32,
        active_displays: *mut CGDirectDisplayID,
        display_count: *mut u32,
    ) -> CGError;
    fn CGSetDisplayTransferByFormula(
        display: CGDirectDisplayID,
        red_min: CGGammaValue,
        red_max: CGGammaValue,
        red_gamma: CGGammaValue,
        green_min: CGGammaValue,
        green_max: CGGammaValue,
        green_gamma: CGGammaValue,
        blue_min: CGGammaValue,
        blue_max: CGGammaValue,
        blue_gamma: CGGammaValue,
    ) -> CGError;
    fn CGDisplayRestoreColorSyncSettings();
}

// ── State ───────────────────────────────────────────────────────────────────

struct DimState {
    active: bool,
    /// Per-display opacity that is currently applied.
    applied: HashMap<CGDirectDisplayID, f32>,
}

static DIM_STATE: LazyLock<Mutex<DimState>> = LazyLock::new(|| {
    Mutex::new(DimState {
        active: false,
        applied: HashMap::new(),
    })
});

// ── Public API ──────────────────────────────────────────────────────────────

/// Show (apply) dimming on screens.
/// When multi_monitor is false, only the primary display is dimmed.
/// When multi_monitor is true, all displays are dimmed with per-display opacity.
pub fn show(
    mtm: MainThreadMarker,
    global_opacity: f32,
    multi_monitor: bool,
    per_display: &HashMap<String, f32>,
) {
    let displays = active_displays();
    let names = screen_names(mtm);
    let display_ids = display_ids_for_screens(mtm);
    let main_id = unsafe { CGMainDisplayID() };

    let mut state = DIM_STATE.lock().unwrap();

    // Restore all displays first to avoid stale gamma
    unsafe {
        CGDisplayRestoreColorSyncSettings();
    }
    state.applied.clear();

    for &did in displays.iter() {
        // Single-monitor mode: only dim the primary display
        if !multi_monitor && did != main_id {
            continue;
        }

        let opacity = if multi_monitor {
            // Try to find by display name
            let name = display_ids
                .iter()
                .find(|(id, _)| *id == did)
                .and_then(|(_, idx)| names.get(*idx as usize));
            name.and_then(|n| per_display.get(n))
                .copied()
                .unwrap_or(global_opacity)
        } else {
            global_opacity
        };

        apply_gamma(did, opacity);
        state.applied.insert(did, opacity);
    }

    state.active = true;
    eprintln!(
        "SaveMyEyes: Gamma dimming applied to {} display(s).",
        state.applied.len()
    );
}

/// Remove dimming from all displays.
pub fn hide() {
    let mut state = DIM_STATE.lock().unwrap();
    if state.active {
        unsafe {
            CGDisplayRestoreColorSyncSettings();
        }
        state.applied.clear();
        state.active = false;
        eprintln!("SaveMyEyes: Gamma restored on all displays.");
    }
}

/// Update opacity on already-dimmed displays without a full reset cycle.
/// Returns false if dimming is not active (caller should use show()).
pub fn update_opacity(
    mtm: MainThreadMarker,
    global_opacity: f32,
    multi_monitor: bool,
    per_display: &HashMap<String, f32>,
) -> bool {
    let state = DIM_STATE.lock().unwrap();
    if !state.active {
        return false;
    }
    drop(state); // Release lock before calling show()

    // Re-apply with new values
    show(mtm, global_opacity, multi_monitor, per_display);
    true
}

/// Set opacity on all dimmed displays (single-monitor shorthand).
#[allow(dead_code)]
pub fn set_opacity(opacity: f32) {
    let state = DIM_STATE.lock().unwrap();
    if !state.active {
        return;
    }
    let applied_clone: Vec<CGDirectDisplayID> = state.applied.keys().copied().collect();
    drop(state);
    for did in applied_clone {
        apply_gamma(did, opacity);
    }
}

/// Set opacity on a specific monitor by index.
#[allow(dead_code)]
pub fn set_monitor_opacity(monitor_index: u32, opacity: f32) {
    let displays = active_displays();
    if let Some(&did) = displays.get(monitor_index as usize) {
        apply_gamma(did, opacity);
    }
}

/// Check if dimming is active.
pub fn is_visible() -> bool {
    DIM_STATE.lock().unwrap().active
}

/// Re-apply gamma after a Space change or wake from sleep.
/// macOS can reset gamma tables during Space transitions.
pub fn reorder_front() {
    let state = DIM_STATE.lock().unwrap();
    if !state.active {
        return;
    }
    let snapshot: Vec<(CGDirectDisplayID, f32)> =
        state.applied.iter().map(|(&k, &v)| (k, v)).collect();
    drop(state);
    for (did, opacity) in snapshot {
        apply_gamma(did, opacity);
    }
}

/// Get the screen index that contains the given point (mouse cursor).
#[allow(dead_code)]
pub fn screen_index_at_point(mtm: MainThreadMarker, x: f64, y: f64) -> u32 {
    let screens = NSScreen::screens(mtm);
    let count = screens.count() as usize;
    for i in 0..count {
        let screen = screens.objectAtIndex(i as NSUInteger);
        let frame = screen.frame();
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
pub fn screen_names(mtm: MainThreadMarker) -> Vec<String> {
    let screens = NSScreen::screens(mtm);
    let count = screens.count() as usize;
    let mut names = Vec::with_capacity(count);
    for i in 0..count {
        let screen = screens.objectAtIndex(i as NSUInteger);
        let name = screen.localizedName().to_string();
        let final_name = if names.contains(&name) {
            format!("{} ({})", name, i + 1)
        } else {
            name
        };
        names.push(final_name);
    }
    names
}

/// Refresh dimming (e.g. after screen config changes).
#[allow(dead_code)]
pub fn refresh(
    mtm: MainThreadMarker,
    global_opacity: f32,
    multi_monitor: bool,
    per_display: &HashMap<String, f32>,
) {
    if is_visible() {
        show(mtm, global_opacity, multi_monitor, per_display);
    }
}

// ── Internal ────────────────────────────────────────────────────────────────

/// Apply gamma reduction on a single display.
/// opacity 0.0 = no dimming, 0.9 = 90% dimmed.
fn apply_gamma(display: CGDirectDisplayID, opacity: f32) {
    let max = (1.0 - opacity).clamp(0.05, 1.0); // Never go fully black
    unsafe {
        CGSetDisplayTransferByFormula(
            display,
            0.0, max, 1.0, // Red:   min, max, gamma
            0.0, max, 1.0, // Green: min, max, gamma
            0.0, max, 1.0, // Blue:  min, max, gamma
        );
    }
}

/// List all active (online) CGDirectDisplayIDs.
fn active_displays() -> Vec<CGDirectDisplayID> {
    let mut count: u32 = 0;
    unsafe {
        CGGetActiveDisplayList(0, std::ptr::null_mut(), &mut count);
    }
    if count == 0 {
        return vec![];
    }
    let mut ids = vec![0u32; count as usize];
    unsafe {
        CGGetActiveDisplayList(count, ids.as_mut_ptr(), &mut count);
    }
    ids.truncate(count as usize);
    ids
}

/// Map NSScreen index → CGDirectDisplayID via deviceDescription["NSScreenNumber"].
fn display_ids_for_screens(mtm: MainThreadMarker) -> Vec<(CGDirectDisplayID, u32)> {
    let screens = NSScreen::screens(mtm);
    let count = screens.count() as usize;
    let mut result = Vec::with_capacity(count);
    for i in 0..count {
        let screen = screens.objectAtIndex(i as NSUInteger);
        let desc = screen.deviceDescription();
        let key = objc2_foundation::NSString::from_str("NSScreenNumber");
        if let Some(val) = desc.objectForKey(&key) {
            let did: u32 = unsafe { objc2::msg_send![&*val, unsignedIntValue] };
            result.push((did, i as u32));
        }
    }
    result
}
