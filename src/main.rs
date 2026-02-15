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
use windows::Win32::System::Threading::{CreateMutexW, OpenMutexW, SYNCHRONIZATION_ACCESS_RIGHTS};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, TranslateMessage, MSG,
};

const SINGLE_INSTANCE_MUTEX: &str = "SaveMyEyesMutex\0";

fn main() {
    // Single-instance check
    if is_already_running() {
        return;
    }

    // Clean up .old exe from a previous self-update
    updater::cleanup_old_exe();

    // Check if we were just updated
    let just_updated = updater::was_just_updated();

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
            if cfg.multi_monitor {
                overlay::set_per_monitor_opacities(&cfg.per_monitor_opacity);
            }
            overlay::show_overlay(cfg.opacity, false);
        }
    }

    // Show and focus main window on startup
    ui::show_window(hwnd);

    // Show "just updated" toast if relaunched after self-update
    if just_updated {
        ui::show_toast(hwnd, &format!("Updated to v{} successfully!", updater::APP_VERSION));
    }

    // Auto-check for updates in background (silent, after 5 seconds)
    {
        let config_clone = config.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(5));
            let auto_update = config_clone.lock().unwrap().auto_update;
            if auto_update {
                let result = updater::check_for_update(updater::APP_VERSION);
                if let updater::UpdateResult::UpdateAvailable { version, download_url, .. } = result {
                    // Ask user if they want to auto-download
                    if updater::prompt_update_dialog(&version) {
                        match updater::download_update(&download_url) {
                            Ok(path) => {
                                let _ = updater::apply_update_and_relaunch(&path);
                            }
                            Err(_e) => {
                                // Fallback: open releases page in browser
                                updater::open_url("https://github.com/KDSPL/savemyeyes/releases");
                            }
                        }
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
        if cfg.multi_monitor {
            overlay::set_per_monitor_opacities(&cfg.per_monitor_opacity);
        }
        overlay::show_overlay(cfg.opacity, false);
    }
}

/// Get the cursor position
fn get_cursor_pos() -> (i32, i32) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut pt = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
    }
    (pt.x, pt.y)
}

/// Adjust opacity by delta (called from hotkey handler)
pub fn do_adjust_opacity(config: &Arc<Mutex<AppConfig>>, delta: f32) {
    let mut cfg = config.lock().unwrap();

    if cfg.multi_monitor {
        // Multi-monitor mode: adjust only the monitor under the cursor
        let (cx, cy) = get_cursor_pos();
        let mon_idx = overlay::get_monitor_index_at_point(cx, cy);

        let was_disabled = !cfg.is_enabled;
        if was_disabled {
            cfg.is_enabled = true;
            cfg.opacity = cfg.last_opacity;
        }

        let current = cfg.per_monitor_opacity.get(&mon_idx).copied().unwrap_or(cfg.opacity);
        let new_opacity = (current + delta).clamp(0.0, 0.9);
        cfg.per_monitor_opacity.insert(mon_idx, new_opacity);
        config::save_config(&cfg);

        if was_disabled {
            overlay::set_per_monitor_opacities(&cfg.per_monitor_opacity);
            overlay::show_overlay(cfg.opacity, false);
        } else if overlay::is_visible() {
            overlay::set_monitor_opacity(mon_idx, new_opacity);
        }
    } else {
        // Single-monitor mode: original behavior
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
}
