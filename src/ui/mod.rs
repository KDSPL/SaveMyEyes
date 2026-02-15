pub mod controls;
pub mod painting;
pub mod theme;

use controls::*;
use theme::*;

use crate::config::{self, AppConfig};
use crate::{autostart, overlay, tray, updater};

use std::sync::{Arc, Mutex};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::*;

const CLASS_NAME: &str = "SaveMyEyesSettingsWnd\0";
const WM_TRAY_CALLBACK: u32 = tray::WM_TRAY_ICON;
const TOAST_TIMER_ID: usize = 100;
const STATUS_CLEAR_TIMER_ID: usize = 101;

/// Shared state pointer stored in GWLP_USERDATA
struct WndState {
    ui: UiState,
    config: Arc<Mutex<AppConfig>>,
}

// Global pointer to WndState (set during window creation, used in WndProc)
static mut WND_STATE: *mut WndState = std::ptr::null_mut();

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Create and return the settings window (initially hidden)
pub fn create_window(config: Arc<Mutex<AppConfig>>) -> HWND {
    let class_name = wide(CLASS_NAME);

    unsafe {
        let hinstance = GetModuleHandleW(PCWSTR::null()).unwrap_or_default();
        let icon_id = PCWSTR(1 as *const u16);
        let hicon = LoadIconW(Some(hinstance.into()), icon_id)
            .ok()
            .or_else(|| LoadIconW(None, IDI_APPLICATION).ok())
            .unwrap_or_default();

        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            hbrBackground: CreateSolidBrush(CLR_BACKGROUND),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hIcon: hicon,
            ..Default::default()
        };

        RegisterClassW(&wc);

        // Calculate window size to get desired client area
        let mut wr = RECT {
            left: 0,
            top: 0,
            right: WINDOW_WIDTH,
            bottom: WINDOW_HEIGHT,
        };
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let _ = AdjustWindowRectEx(&mut wr, style, false, WINDOW_EX_STYLE::default());

        let win_w = wr.right - wr.left;
        let win_h = wr.bottom - wr.top;

        let title = wide("SaveMyEyes by KraftPixel");

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            win_w,
            win_h,
            None,
            None,
            Some(hinstance.into()),
            None,
        )
        .unwrap();

        // Initialize state
        let mut ui = UiState::new();
        {
            let cfg = config.lock().unwrap();
            ui.slider.value = (cfg.opacity * 100.0).round() as i32;
            ui.enabled_toggle.checked = cfg.is_enabled;
            ui.autostart_toggle.checked = cfg.launch_on_login;
            ui.auto_update_toggle.checked = cfg.auto_update;
            ui.shortcut_texts = [
                cfg.hotkey_toggle.clone(),
                cfg.hotkey_increase.clone(),
                cfg.hotkey_decrease.clone(),
            ];
        }
        // Sync autostart toggle with actual registry state
        ui.autostart_toggle.checked = autostart::is_enabled();

        let wnd_state = Box::new(WndState { ui, config });

        WND_STATE = Box::into_raw(wnd_state);

        hwnd
    }
}

/// Show and focus the settings window
pub fn show_window(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

/// Hide the settings window
pub fn hide_window(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

/// Trigger a repaint
pub fn invalidate(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, true);
    }
}

/// Update UI state from config (called when hotkeys change things)
pub fn sync_from_config(hwnd: HWND) {
    unsafe {
        if WND_STATE.is_null() {
            return;
        }
        let state = &mut *WND_STATE;
        let cfg = state.config.lock().unwrap();
        state.ui.slider.value = (cfg.opacity * 100.0).round() as i32;
        state.ui.enabled_toggle.checked = cfg.is_enabled;
        drop(cfg);
        invalidate(hwnd);
    }
}

/// Show a toast message
pub fn show_toast(hwnd: HWND, message: &str) {
    unsafe {
        if WND_STATE.is_null() {
            return;
        }
        let state = &mut *WND_STATE;
        state.ui.toast_message = message.to_string();
        state.ui.toast_visible = true;
        invalidate(hwnd);

        // Auto-hide after 2 seconds
        SetTimer(Some(hwnd), TOAST_TIMER_ID, 2000, None);
    }
}

/// Window procedure
unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            // Double-buffer to avoid flicker
            let mut client = RECT::default();
            let _ = GetClientRect(hwnd, &mut client);

            let mem_dc = CreateCompatibleDC(Some(hdc));
            let mem_bmp = CreateCompatibleBitmap(hdc, client.right, client.bottom);
            let old_bmp = SelectObject(mem_dc, HGDIOBJ::from(mem_bmp));

            if !WND_STATE.is_null() {
                let state = &mut *WND_STATE;
                painting::paint(mem_dc, &client, &mut state.ui);
            }

            // Blit to screen
            let _ = BitBlt(
                hdc,
                0,
                0,
                client.right,
                client.bottom,
                Some(mem_dc),
                0,
                0,
                SRCCOPY,
            );

            SelectObject(mem_dc, old_bmp);
            let _ = DeleteObject(HGDIOBJ::from(mem_bmp));
            let _ = DeleteDC(mem_dc);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }

        WM_LBUTTONDOWN => {
            if WND_STATE.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let state = &mut *WND_STATE;
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

            // Tab clicks
            for i in 0..3 {
                if point_in_rect(x, y, &state.ui.tab_rects[i]) {
                    state.ui.active_tab = match i {
                        0 => Tab::Dimmer,
                        1 => Tab::Settings,
                        _ => Tab::Shortcuts,
                    };
                    invalidate(hwnd);
                    return LRESULT(0);
                }
            }

            // Slider drag
            if state.ui.active_tab == Tab::Dimmer
                && point_in_rect(x, y, &state.ui.slider.thumb_rect)
            {
                state.ui.slider.dragging = true;
                SetCapture(hwnd);
                let val = state.ui.slider.value_from_x(x);
                state.ui.slider.value = val;
                invalidate(hwnd);
                return LRESULT(0);
            }

            // Dimmer toggle
            if state.ui.active_tab == Tab::Dimmer
                && point_in_rect(x, y, &state.ui.enabled_toggle.rect)
            {
                state.ui.enabled_toggle.checked = !state.ui.enabled_toggle.checked;
                let enabled = state.ui.enabled_toggle.checked;
                {
                    let mut cfg = state.config.lock().unwrap();
                    cfg.is_enabled = enabled;
                    config::save_config(&cfg);
                    if enabled {
                        overlay::show_overlay(cfg.opacity, false);
                    } else {
                        overlay::hide_overlay();
                    }
                }
                show_toast(
                    hwnd,
                    if enabled {
                        "Dimmer enabled"
                    } else {
                        "Dimmer disabled"
                    },
                );
                invalidate(hwnd);
                return LRESULT(0);
            }

            // Settings tab toggles
            if state.ui.active_tab == Tab::Settings {
                // Autostart toggle
                if point_in_rect(x, y, &state.ui.autostart_toggle.rect) {
                    state.ui.autostart_toggle.checked = !state.ui.autostart_toggle.checked;
                    let enabled = state.ui.autostart_toggle.checked;
                    if enabled {
                        autostart::enable();
                    } else {
                        autostart::disable();
                    }
                    {
                        let mut cfg = state.config.lock().unwrap();
                        cfg.launch_on_login = enabled;
                        config::save_config(&cfg);
                    }
                    show_toast(hwnd, "Autostart setting saved");
                    invalidate(hwnd);
                    return LRESULT(0);
                }

                // Auto-update toggle
                if point_in_rect(x, y, &state.ui.auto_update_toggle.rect) {
                    state.ui.auto_update_toggle.checked = !state.ui.auto_update_toggle.checked;
                    let enabled = state.ui.auto_update_toggle.checked;
                    {
                        let mut cfg = state.config.lock().unwrap();
                        cfg.auto_update = enabled;
                        config::save_config(&cfg);
                    }
                    show_toast(
                        hwnd,
                        if enabled {
                            "Auto-update enabled"
                        } else {
                            "Auto-update disabled"
                        },
                    );
                    invalidate(hwnd);
                    return LRESULT(0);
                }

                // Check Now button
                if point_in_rect(x, y, &state.ui.check_update_btn.rect)
                    && !state.ui.check_update_btn.disabled
                {
                    state.ui.check_update_btn.disabled = true;
                    state.ui.update_status_text = "Checking...".into();
                    invalidate(hwnd);

                    // Run update check in background thread
                    let hwnd_val = hwnd.0 as isize;
                    std::thread::spawn(move || {
                        let result = updater::check_for_update("0.9.0");
                        let msg_code = match result {
                            updater::UpdateResult::NoUpdate => 0isize,
                            updater::UpdateResult::UpdateAvailable { url, .. } => {
                                updater::open_url(&url);
                                1
                            }
                            updater::UpdateResult::Error(_) => 2,
                        };
                        unsafe {
                            let _ = PostMessageW(
                                Some(HWND(hwnd_val as *mut _)),
                                WM_APP + 10,
                                WPARAM(msg_code as usize),
                                LPARAM(0),
                            );
                        }
                    });
                    return LRESULT(0);
                }
            }

            // Credit link
            if point_in_rect(x, y, &state.ui.credit_rect) {
                updater::open_url("https://kraftpixel.com");
                return LRESULT(0);
            }

            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        WM_LBUTTONUP => {
            if !WND_STATE.is_null() {
                let state = &mut *WND_STATE;
                if state.ui.slider.dragging {
                    state.ui.slider.dragging = false;
                    let _ = ReleaseCapture();

                    let val = state.ui.slider.value;
                    {
                        let mut cfg = state.config.lock().unwrap();
                        cfg.opacity = val as f32 / 100.0;
                        config::save_config(&cfg);
                        if overlay::is_visible() {
                            overlay::set_opacity(cfg.opacity);
                        }
                        // Auto-enable dimmer when user adjusts slider
                        if !cfg.is_enabled && val > 0 {
                            cfg.is_enabled = true;
                            state.ui.enabled_toggle.checked = true;
                            config::save_config(&cfg);
                            overlay::show_overlay(cfg.opacity, false);
                        }
                    }
                    show_toast(hwnd, "Opacity updated");
                    invalidate(hwnd);
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        WM_MOUSEMOVE => {
            if !WND_STATE.is_null() {
                let state = &mut *WND_STATE;
                let x = (lparam.0 & 0xFFFF) as i16 as i32;

                if state.ui.slider.dragging {
                    let val = state.ui.slider.value_from_x(x);
                    state.ui.slider.value = val;

                    // Live update overlay opacity while dragging
                    {
                        let cfg = state.config.lock().unwrap();
                        if overlay::is_visible() || cfg.is_enabled {
                            overlay::set_opacity(val as f32 / 100.0);
                        }
                    }

                    invalidate(hwnd);
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        WM_COMMAND => {
            let cmd = (wparam.0 & 0xFFFF) as u32;
            match cmd {
                tray::IDM_TOGGLE => {
                    if !WND_STATE.is_null() {
                        let state = &mut *WND_STATE;
                        let mut cfg = state.config.lock().unwrap();
                        cfg.is_enabled = !cfg.is_enabled;
                        state.ui.enabled_toggle.checked = cfg.is_enabled;
                        config::save_config(&cfg);
                        if cfg.is_enabled {
                            overlay::show_overlay(cfg.opacity, false);
                        } else {
                            overlay::hide_overlay();
                        }
                        drop(cfg);
                        invalidate(hwnd);
                    }
                }
                tray::IDM_SETTINGS => {
                    show_window(hwnd);
                }
                tray::IDM_QUIT => {
                    tray::remove_tray_icon(hwnd);
                    PostQuitMessage(0);
                }
                _ => {}
            }
            LRESULT(0)
        }

        WM_TRAY_CALLBACK => {
            let event = (lparam.0 & 0xFFFF) as u32;
            match event {
                WM_LBUTTONUP => {
                    show_window(hwnd);
                }
                WM_RBUTTONUP => {
                    tray::show_context_menu(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }

        WM_HOTKEY => {
            if !WND_STATE.is_null() {
                let state = &mut *WND_STATE;
                let id = wparam.0 as i32;
                match id {
                    crate::hotkeys::HOTKEY_TOGGLE => {
                        crate::do_toggle_dimmer(&state.config);
                        sync_from_config(hwnd);
                    }
                    crate::hotkeys::HOTKEY_INCREASE => {
                        crate::do_adjust_opacity(&state.config, 0.1);
                        sync_from_config(hwnd);
                    }
                    crate::hotkeys::HOTKEY_DECREASE => {
                        crate::do_adjust_opacity(&state.config, -0.1);
                        sync_from_config(hwnd);
                    }
                    _ => {}
                }
            }
            LRESULT(0)
        }

        WM_TIMER => {
            let timer_id = wparam.0;
            if timer_id == TOAST_TIMER_ID {
                if !WND_STATE.is_null() {
                    let state = &mut *WND_STATE;
                    state.ui.toast_visible = false;
                    state.ui.toast_message.clear();
                    let _ = KillTimer(Some(hwnd), TOAST_TIMER_ID);
                    invalidate(hwnd);
                }
            } else if timer_id == STATUS_CLEAR_TIMER_ID {
                if !WND_STATE.is_null() {
                    let state = &mut *WND_STATE;
                    state.ui.update_status_text.clear();
                    let _ = KillTimer(Some(hwnd), STATUS_CLEAR_TIMER_ID);
                    invalidate(hwnd);
                }
            }
            LRESULT(0)
        }

        // Update check result callback
        x if x == WM_APP + 10 => {
            if !WND_STATE.is_null() {
                let state = &mut *WND_STATE;
                state.ui.check_update_btn.disabled = false;
                match wparam.0 {
                    0 => {
                        state.ui.update_status_text = "You're on the latest version!".into();
                        show_toast(hwnd, "No updates available");
                    }
                    1 => {
                        state.ui.update_status_text = "Opening download page...".into();
                        show_toast(hwnd, "Update available!");
                    }
                    _ => {
                        state.ui.update_status_text = "Update check failed".into();
                        show_toast(hwnd, "Update check failed");
                    }
                }
                invalidate(hwnd);

                // Clear status after 5 seconds
                SetTimer(Some(hwnd), STATUS_CLEAR_TIMER_ID, 5000, None);
            }
            LRESULT(0)
        }

        WM_CLOSE => {
            // Hide to tray instead of quitting
            hide_window(hwnd);
            LRESULT(0)
        }

        WM_DESTROY => {
            // Cleanup
            if !WND_STATE.is_null() {
                let _ = Box::from_raw(WND_STATE);
                WND_STATE = std::ptr::null_mut();
            }
            PostQuitMessage(0);
            LRESULT(0)
        }

        WM_ERASEBKGND => {
            // Handled in WM_PAINT with double buffering
            LRESULT(1)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
