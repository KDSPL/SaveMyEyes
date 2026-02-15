// macOS application lifecycle — NSApplication setup, menu bar, and run loop.
//
// This is the entry point for the Cocoa application. It:
//   • Creates the NSApplication singleton
//   • Sets up the app delegate
//   • Installs the status bar item (tray icon)
//   • Starts the run loop

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, AllocAnyThread, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate};
use objc2_foundation::{NSNotification, NSObject, NSObjectProtocol};

use std::sync::{Arc, Mutex};

use crate::config;
use crate::hotkeys;
use crate::hotkeys::HotkeyAction;
use crate::overlay;
use crate::tray;

/// Shared application state accessible from callbacks
pub struct AppState {
    pub config: config::AppConfig,
}

static mut APP_STATE: Option<Arc<Mutex<AppState>>> = None;

pub fn state() -> Arc<Mutex<AppState>> {
    unsafe { APP_STATE.clone().expect("AppState not initialized") }
}

/// Called from the hotkey event tap thread — dispatches to the main thread.
pub fn dispatch_hotkey(action: HotkeyAction) {
    // Perform on main thread via dispatch
    std::thread::spawn(move || {
        let st = state();
        let mut s = st.lock().unwrap();
        match action {
            HotkeyAction::Toggle => {
                s.config.is_enabled = !s.config.is_enabled;
                config::save_config(&s.config);
                // overlay show/hide must happen on main thread;
                // for now we set a flag and the next run-loop cycle picks it up.
            }
            HotkeyAction::Increase => {
                if s.config.multi_monitor {
                    // Adjust the monitor under the cursor
                    // (requires main-thread NSEvent access — simplified here)
                    let global_new = (s.config.opacity + 0.1).clamp(0.0, 0.9);
                    s.config.opacity = global_new;
                } else {
                    s.config.opacity = (s.config.opacity + 0.1).clamp(0.0, 0.9);
                }
                s.config.is_enabled = true;
                config::save_config(&s.config);
            }
            HotkeyAction::Decrease => {
                if s.config.multi_monitor {
                    let global_new = (s.config.opacity - 0.1).clamp(0.0, 0.9);
                    s.config.opacity = global_new;
                } else {
                    s.config.opacity = (s.config.opacity - 0.1).clamp(0.0, 0.9);
                }
                s.config.is_enabled = true;
                config::save_config(&s.config);
            }
        }
    });
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "SaveMyEyesAppDelegate"]
    #[thread_kind = AllocAnyThread]
    struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            let mtm = MainThreadMarker::from(self);

            let cfg = config::load_config();
            let state = Arc::new(Mutex::new(AppState { config: cfg.clone() }));
            unsafe {
                APP_STATE = Some(state.clone());
            }

            // Setup system tray (status bar item)
            tray::setup(mtm);

            // Register global hotkeys
            hotkeys::register_all();

            // Show overlay if enabled
            if cfg.is_enabled {
                overlay::show(mtm, cfg.opacity, cfg.multi_monitor, &cfg.per_monitor_opacity);
            }
        }
    }
);

impl AppDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc::<Self>();
        unsafe { msg_send![super(this), init] }
    }
}

pub fn run() {
    let mtm = MainThreadMarker::new()
        .expect("SaveMyEyes must be run on the main thread");

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let delegate = AppDelegate::new(mtm);
    let delegate_proto = ProtocolObject::from_ref(&*delegate);
    app.setDelegate(Some(delegate_proto));

    unsafe { app.run() };
}
