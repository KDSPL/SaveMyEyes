// Prevents console window in release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod config;
mod hotkeys;
mod overlay;
mod tray;
mod ui;
mod updater;

use config::AppConfig;
use std::sync::{Arc, Mutex};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::{CreateMutexW, OpenMutexW, SYNCHRONIZATION_ACCESS_RIGHTS};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PostMessageW, TranslateMessage, MSG, WM_APP,
};

const SINGLE_INSTANCE_MUTEX: &str = "SaveMyEyesMutex\0";

fn main() {
    // Single-instance check
    if is_already_running() {
        return;
    }

    // Load config
    let cfg = config::load_config();
    let config = Arc::new(Mutex::new(cfg));

    // Create the settings window
    let hwnd = ui::create_window(config.clone());

    // Setup system tray
    tray::add_tray_icon(hwnd);

    // Register global hotkeys
    hotkeys::register_all(hwnd);

    // Show overlay if enabled
    {
        let cfg = config.lock().unwrap();
        if cfg.is_enabled {
            overlay::show_overlay(cfg.opacity, false);
        }
    }

    // Show and focus main window on startup
    ui::show_window(hwnd);

    // Auto-check for updates in background (silent, after 5 seconds)
    {
        let config_clone = config.clone();
        let hwnd_val = hwnd.0 as isize;
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(5));
            let auto_update = config_clone.lock().unwrap().auto_update;
            if auto_update {
                let result = updater::check_for_update("0.9.0");
                if let updater::UpdateResult::UpdateAvailable { .. } = result {
                    unsafe {
                        let _ = PostMessageW(
                            Some(HWND(hwnd_val as *mut _)),
                            WM_APP + 10,
                            WPARAM(1),
                            LPARAM(0),
                        );
                    }
                }
            }
        });
    }

    // Win32 message loop
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    // Cleanup
    hotkeys::unregister_all(hwnd);
    tray::remove_tray_icon(hwnd);
    overlay::hide_overlay();
}

/// Check if another instance is already running
fn is_already_running() -> bool {
    let name: Vec<u16> = SINGLE_INSTANCE_MUTEX.encode_utf16().collect();

    unsafe {
        // Try to open existing mutex
        let existing = OpenMutexW(
            SYNCHRONIZATION_ACCESS_RIGHTS(0x001F0001), // MUTEX_ALL_ACCESS
            false,
            PCWSTR(name.as_ptr()),
        );
        if existing.is_ok() {
            // Another instance exists
            return true;
        }

        // Create the mutex (this instance owns it)
        let _ = CreateMutexW(None, true, PCWSTR(name.as_ptr()));
        false
    }
}

/// Toggle dimmer on/off (called from hotkey handler)
pub fn do_toggle_dimmer(config: &Arc<Mutex<AppConfig>>) {
    let mut cfg = config.lock().unwrap();

    if cfg.is_enabled {
        // Turning OFF
        if cfg.opacity > 0.0 {
            cfg.last_opacity = cfg.opacity;
        }
        cfg.is_enabled = false;
        cfg.opacity = 0.0;
        config::save_config(&cfg);
        overlay::hide_overlay();
    } else {
        // Turning ON
        cfg.is_enabled = true;
        cfg.opacity = cfg.last_opacity;
        config::save_config(&cfg);
        overlay::show_overlay(cfg.opacity, false);
    }
}

/// Adjust opacity by delta (called from hotkey handler)
pub fn do_adjust_opacity(config: &Arc<Mutex<AppConfig>>, delta: f32) {
    let mut cfg = config.lock().unwrap();

    let was_disabled = !cfg.is_enabled;
    if was_disabled {
        cfg.is_enabled = true;
        cfg.opacity = cfg.last_opacity;
    }

    let new_opacity = (cfg.opacity + delta).clamp(0.0, 0.9);
    cfg.opacity = new_opacity;

    if new_opacity > 0.0 {
        cfg.last_opacity = new_opacity;
    }
    config::save_config(&cfg);

    if was_disabled {
        overlay::show_overlay(cfg.opacity, false);
    } else if overlay::is_visible() {
        overlay::set_opacity(cfg.opacity);
    }
}
