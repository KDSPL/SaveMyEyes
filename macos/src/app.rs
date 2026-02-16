// macOS application lifecycle — NSApplication setup, menu bar, and run loop.
//
// This is the entry point for the Cocoa application. It:
//   • Creates the NSApplication singleton
//   • Sets up the app delegate
//   • Installs the status bar item (tray icon)
//   • Registers global hotkeys
//   • Dispatches hotkey actions on the main thread via GCD
//   • Runs auto-update check on launch
//   • Starts the run loop

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate};
use objc2_foundation::{NSNotification, NSObject, NSObjectProtocol, NSString};

use std::sync::{Arc, Mutex, OnceLock};

use crate::config;
use crate::hotkeys;
use crate::hotkeys::HotkeyAction;
use crate::overlay;
use crate::tray;
use crate::updater;

/// Shared application state accessible from callbacks
pub struct AppState {
    pub config: config::AppConfig,
}

static APP_STATE: OnceLock<Arc<Mutex<AppState>>> = OnceLock::new();

pub fn state() -> Arc<Mutex<AppState>> {
    APP_STATE.get().expect("AppState not initialized").clone()
}

// ── GCD dispatch helpers ────────────────────────────────────────────────────

#[allow(non_camel_case_types)]
type dispatch_queue_t = *const std::ffi::c_void;
#[allow(non_camel_case_types)]
type dispatch_function_t = unsafe extern "C" fn(*mut std::ffi::c_void);

extern "C" {
    // `dispatch_get_main_queue()` is a macro that returns `&_dispatch_main_q`.
    // We link to the underlying symbol directly.
    static _dispatch_main_q: std::ffi::c_void;

    fn dispatch_async_f(
        queue: dispatch_queue_t,
        context: *mut std::ffi::c_void,
        work: dispatch_function_t,
    );
}

/// Run a closure on the main thread via GCD dispatch_async.
pub fn run_on_main<F: FnOnce() + Send + 'static>(f: F) {
    let boxed: Box<Box<dyn FnOnce() + Send>> = Box::new(Box::new(f));
    let raw = Box::into_raw(boxed) as *mut std::ffi::c_void;
    unsafe {
        let main_queue: dispatch_queue_t = &_dispatch_main_q as *const _ as dispatch_queue_t;
        dispatch_async_f(main_queue, raw, trampoline);
    }

    unsafe extern "C" fn trampoline(ctx: *mut std::ffi::c_void) {
        let closure: Box<Box<dyn FnOnce() + Send>> = Box::from_raw(ctx as *mut _);
        closure();
    }
}

/// Called from the hotkey event tap thread — dispatches to the main thread via GCD.
pub fn dispatch_hotkey(action: HotkeyAction) {
    run_on_main(move || {
        let mtm = MainThreadMarker::new().unwrap();
        eprintln!("SaveMyEyes: dispatch_hotkey({:?})", action);

        {
            let st = state();
            let mut s = st.lock().unwrap();

            match action {
                HotkeyAction::Toggle => {
                    if s.config.is_enabled {
                        s.config.last_opacity = s.config.opacity;
                        s.config.is_enabled = false;
                        config::save_config(&s.config);
                        overlay::hide();
                    } else {
                        s.config.opacity = s.config.last_opacity;
                        s.config.is_enabled = true;
                        config::save_config(&s.config);
                        overlay::show(
                            mtm,
                            s.config.opacity,
                            s.config.multi_monitor,
                            &s.config.per_display_opacity,
                        );
                    }
                }
                HotkeyAction::Increase => {
                    // Find which monitor the cursor is on
                    let mouse_loc: objc2_foundation::NSPoint = unsafe {
                        objc2::msg_send![objc2::runtime::AnyClass::get(c"NSEvent").unwrap(), mouseLocation]
                    };
                    let active_idx = if s.config.multi_monitor {
                        overlay::screen_index_at_point(mtm, mouse_loc.x, mouse_loc.y)
                    } else {
                        0
                    };
                    let names = overlay::screen_names(mtm);
                    let display_name = names.get(active_idx as usize).cloned().unwrap_or_default();
                    let cur = s.config.per_display_opacity.get(&display_name).copied().unwrap_or(s.config.opacity);
                    let new_op = (cur + 0.1).clamp(0.0, 0.9);
                    s.config.per_display_opacity.insert(display_name, new_op);
                    if active_idx == 0 {
                        s.config.opacity = new_op;
                    }
                    s.config.is_enabled = true;
                    config::save_config(&s.config);
                    if !overlay::update_opacity(mtm, s.config.opacity, s.config.multi_monitor, &s.config.per_display_opacity) {
                        overlay::show(
                            mtm,
                            s.config.opacity,
                            s.config.multi_monitor,
                            &s.config.per_display_opacity,
                        );
                    }
                }
                HotkeyAction::Decrease => {
                    // Find which monitor the cursor is on
                    let mouse_loc: objc2_foundation::NSPoint = unsafe {
                        objc2::msg_send![objc2::runtime::AnyClass::get(c"NSEvent").unwrap(), mouseLocation]
                    };
                    let active_idx = if s.config.multi_monitor {
                        overlay::screen_index_at_point(mtm, mouse_loc.x, mouse_loc.y)
                    } else {
                        0
                    };
                    let names = overlay::screen_names(mtm);
                    let display_name = names.get(active_idx as usize).cloned().unwrap_or_default();
                    let cur = s.config.per_display_opacity.get(&display_name).copied().unwrap_or(s.config.opacity);
                    let new_op = (cur - 0.1).clamp(0.0, 0.9);
                    s.config.per_display_opacity.insert(display_name, new_op);
                    if active_idx == 0 {
                        s.config.opacity = new_op;
                    }
                    s.config.is_enabled = true;
                    config::save_config(&s.config);
                    if !overlay::update_opacity(mtm, s.config.opacity, s.config.multi_monitor, &s.config.per_display_opacity) {
                        overlay::show(
                            mtm,
                            s.config.opacity,
                            s.config.multi_monitor,
                            &s.config.per_display_opacity,
                        );
                    }
                }
            }
        } // <-- APP_STATE lock is dropped here, BEFORE update_menu

        // Update tray to reflect new state
        tray::update_menu(mtm);

        // Update settings UI (toggle/slider) if open
        crate::ui::update_ui();
    });
}

/// Run the auto-update check after a delay in a background thread.
pub fn schedule_update_check() {
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(5));
        let st = state();
        let auto = st.lock().unwrap().config.auto_update;
        if !auto {
            return;
        }
        let result = updater::check_for_update(updater::APP_VERSION);
        match result {
            updater::UpdateResult::UpdateAvailable {
                version,
                download_url,
                ..
            } => {
                run_on_main(move || {
                    crate::ui::prompt_update(&version, &download_url);
                });
            }
            _ => {}
        }
    });
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "SaveMyEyesAppDelegate"]
    #[thread_kind = MainThreadOnly]
    struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            let mtm = MainThreadMarker::from(self);

            let cfg = config::load_config();
            let state = Arc::new(Mutex::new(AppState { config: cfg.clone() }));
            APP_STATE.set(state.clone()).ok();

            // Setup system tray (status bar item)
            tray::setup(mtm);

            // Register global hotkeys
            hotkeys::register_all();

            // Always ensure accessibility permission is granted.
            // After an update the TCC entry is reset by the update script,
            // but we also prompt on every cold start so the user never has
            // to manually toggle the entry in System Settings.
            hotkeys::request_accessibility_if_needed();

            // Show overlay if enabled
            if cfg.is_enabled {
                overlay::show(mtm, cfg.opacity, cfg.multi_monitor, &cfg.per_display_opacity);
            }

            // Register for screen configuration changes (monitor connect/disconnect)
            unsafe {
                use objc2_foundation::NSNotificationCenter;
                let center = NSNotificationCenter::defaultCenter();
                let name = NSString::from_str("NSApplicationDidChangeScreenParametersNotification");
                center.addObserver_selector_name_object(
                    self,
                    sel!(screenParametersChanged:),
                    Some(&name),
                    None,
                );
            }

            // Schedule auto-update check
            schedule_update_check();
        }

        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        fn should_handle_reopen(&self, _sender: &NSApplication, _has_visible: bool) -> bool {
            // Always open settings when dock icon is clicked
            // (overlay windows count as "visible", so we ignore that flag)
            let mtm = MainThreadMarker::from(self);
            crate::ui::show_settings(mtm);
            true
        }

        #[unsafe(method(screenParametersChanged:))]
        fn screen_parameters_changed(&self, _notification: &NSNotification) {
            let mtm = MainThreadMarker::from(self);
            eprintln!("SaveMyEyes: Screen configuration changed (monitor connect/disconnect)");

            // Refresh overlays if visible
            let st = state();
            let cfg = st.lock().unwrap().config.clone();
            if cfg.is_enabled {
                overlay::show(mtm, cfg.opacity, cfg.multi_monitor, &cfg.per_display_opacity);
            }

            // Rebuild the settings UI if it is open
            crate::ui::rebuild_settings(mtm);
        }
    }
);

impl AppDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc::<Self>();
        unsafe { msg_send![this, init] }
    }
}

pub fn run() {
    let mtm = MainThreadMarker::new()
        .expect("SaveMyEyes must be run on the main thread");

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

    let delegate = AppDelegate::new(mtm);
    let delegate_proto = ProtocolObject::from_ref(&*delegate);
    app.setDelegate(Some(delegate_proto));

    app.run();
}
