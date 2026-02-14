use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

mod overlay;
#[cfg(target_os = "windows")]
mod keyboard_hook;

#[cfg(target_os = "windows")]
use overlay::windows as overlay_impl;

/// Application configuration stored in JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub opacity: f32,
    pub is_enabled: bool,
    pub launch_on_login: bool,
    #[serde(default)]
    pub allow_capture: bool,
    /// Last user-set opacity for toggle restore (defaults to opacity if not set)
    #[serde(default = "default_last_opacity")]
    pub last_opacity: f32,
    pub hotkey_toggle: String,
    pub hotkey_increase: String,
    pub hotkey_decrease: String,
    #[serde(default = "default_auto_update")]
    pub auto_update: bool,
}

fn default_auto_update() -> bool {
    true
}

fn default_last_opacity() -> f32 {
    0.3
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            opacity: 0.3,
            is_enabled: true,
            launch_on_login: true,
            allow_capture: false, // Default: exclude from screen capture
            last_opacity: 0.3,
            hotkey_toggle: "Ctrl+Alt+End".into(),
            hotkey_increase: "Ctrl+Alt+Up".into(),
            hotkey_decrease: "Ctrl+Alt+Down".into(),
            auto_update: true,
        }
    }
}

/// Shared application state
pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub overlay_visible: Mutex<bool>,
}

fn config_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("SaveMyEyes").join("config.json")
}

fn load_config() -> AppConfig {
    let path = config_path();
    if path.exists() {
        let data = fs::read_to_string(&path).unwrap_or_default();
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        AppConfig::default()
    }
}

fn save_config(config: &AppConfig) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let data = serde_json::to_string_pretty(config).unwrap_or_default();
    let _ = fs::write(&path, data);
}

/// Tauri command: get current config
#[tauri::command]
fn get_config(state: State<'_, Arc<AppState>>) -> AppConfig {
    state.config.lock().unwrap().clone()
}

/// Tauri command: set opacity
#[tauri::command]
fn set_opacity(opacity: f32, state: State<'_, Arc<AppState>>) {
    let mut config = state.config.lock().unwrap();
    config.opacity = opacity.clamp(0.0, 0.9);
    save_config(&config);
    
    // Update overlay alpha if visible
    #[cfg(target_os = "windows")]
    if overlay_impl::is_visible() {
        overlay_impl::set_opacity(config.opacity);
    }
}

/// Tauri command: toggle dimmer on/off
#[tauri::command]
fn toggle_dimmer(state: State<'_, Arc<AppState>>) -> bool {
    let mut config = state.config.lock().unwrap();
    config.is_enabled = !config.is_enabled;
    save_config(&config);
    let mut visible = state.overlay_visible.lock().unwrap();
    *visible = config.is_enabled;
    
    // Show or hide overlay
    #[cfg(target_os = "windows")]
    {
        if config.is_enabled {
            overlay_impl::show_overlay(config.opacity, false); // Always exclude from capture
        } else {
            overlay_impl::hide_overlay();
        }
    }
    
    config.is_enabled
}

/// Tauri command: set allow capture mode
/// NOTE: Currently disabled - always using WDA_EXCLUDEFROMCAPTURE to hide overlay from screenshots
#[tauri::command]
fn set_allow_capture(allow: bool, state: State<'_, Arc<AppState>>) {
    let mut config = state.config.lock().unwrap();
    config.allow_capture = allow;
    save_config(&config);
    
    // Recreate overlay with new capture setting
    // NOTE: Using false to always exclude from capture
    #[cfg(target_os = "windows")]
    if overlay_impl::is_visible() {
        overlay_impl::hide_overlay();
        overlay_impl::show_overlay(config.opacity, false); // Always exclude from capture
    }
}

/// Tauri command: set auto-update preference
#[tauri::command]
fn set_auto_update(enabled: bool, state: State<'_, Arc<AppState>>) {
    let mut config = state.config.lock().unwrap();
    config.auto_update = enabled;
    save_config(&config);
}

/// Tauri command: check for updates (called from frontend)
#[tauri::command]
async fn check_for_update(app: AppHandle) -> Result<String, String> {
    use tauri_plugin_updater::UpdaterExt;

    let updater = app.updater_builder().build().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => {
            let version = update.version.clone();
            let body = update.body.clone().unwrap_or_default();
            // Download and install
            if let Err(e) = update.download_and_install(|_, _| {}, || {}).await {
                return Err(format!("Failed to install update: {}", e));
            }
            Ok(format!("Update {} installed. Restart to apply.\n\n{}", version, body))
        }
        Ok(None) => Ok("no_update".to_string()),
        Err(e) => {
            let err_str = e.to_string();
            // Treat network errors / 404 (no releases yet) as "no update"
            if err_str.contains("404")
                || err_str.contains("network")
                || err_str.contains("connect")
                || err_str.contains("status code")
            {
                println!("[Updater] No releases found or endpoint unreachable: {}", err_str);
                Ok("no_update".to_string())
            } else {
                Err(format!("Update check failed: {}", err_str))
            }
        }
    }
}

/// Helper to toggle dimmer from shortcut handler
fn do_toggle_dimmer(app: &AppHandle) {
    println!("[DEBUG] do_toggle_dimmer called");
    
    let state: State<Arc<AppState>> = app.state();
    let mut config = state.config.lock().unwrap();
    
    println!("[DEBUG] Current state: is_enabled={}, opacity={:.2}, last_opacity={:.2}", 
             config.is_enabled, config.opacity, config.last_opacity);
    
    if config.is_enabled {
        // Turning OFF: save current opacity as last_opacity, then hide
        println!("[DEBUG] Turning OFF");
        if config.opacity > 0.0 {
            config.last_opacity = config.opacity;
            println!("[DEBUG] Saved last_opacity={:.2}", config.last_opacity);
        }
        config.is_enabled = false;
        config.opacity = 0.0;
        save_config(&config);
        
        #[cfg(target_os = "windows")]
        {
            println!("[DEBUG] Calling hide_overlay");
            overlay_impl::hide_overlay();
        }
        
        println!("[Shortcut] Dimmer OFF (saved level: {:.0}%)", config.last_opacity * 100.0);
    } else {
        // Turning ON: restore to last_opacity
        println!("[DEBUG] Turning ON");
        config.is_enabled = true;
        config.opacity = config.last_opacity;
        println!("[DEBUG] Restored opacity={:.2}", config.opacity);
        save_config(&config);
        
        #[cfg(target_os = "windows")]
        {
            println!("[DEBUG] Calling show_overlay with opacity={:.2}, allow_capture=false (forced)", 
                     config.opacity);
            overlay_impl::show_overlay(config.opacity, false); // Always exclude from capture
        }
        
        println!("[Shortcut] Dimmer ON (restored to {:.0}%)", config.opacity * 100.0);
    }
    
    // Notify frontend to update UI
    println!("[DEBUG] Emitting config-changed event");
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit("config-changed", config.clone());
        println!("[DEBUG] Event emitted successfully");
    } else {
        println!("[DEBUG] Could not find 'main' window");
    }
}

/// Helper to adjust opacity from shortcut handler
fn do_adjust_opacity(app: &AppHandle, delta: f32) {
    let state: State<Arc<AppState>> = app.state();
    let mut config = state.config.lock().unwrap();
    
    // If dimmer is off, turn it on first
    let was_disabled = !config.is_enabled;
    if was_disabled {
        config.is_enabled = true;
        // Start from last_opacity when turning on
        config.opacity = config.last_opacity;
    }
    
    let new_opacity = (config.opacity + delta).clamp(0.0, 0.9);
    config.opacity = new_opacity;
    
    // Save as last_opacity if non-zero (user-set level)
    if new_opacity > 0.0 {
        config.last_opacity = new_opacity;
    }
    save_config(&config);
    
    #[cfg(target_os = "windows")]
    {
        if was_disabled {
            // Show overlay if it was just enabled
            overlay_impl::show_overlay(config.opacity, false); // Always exclude from capture
            println!("[Shortcut] Dimmer ON, opacity: {:.0}%", config.opacity * 100.0);
        } else if overlay_impl::is_visible() {
            overlay_impl::set_opacity(config.opacity);
            println!("[Shortcut] Adjusted opacity: {:.0}%", config.opacity * 100.0);
        }
    }
    
    // Notify frontend to update UI
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit("config-changed", config.clone());
    }
}

/// Register global keyboard shortcuts
fn register_shortcuts(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // Ctrl+Alt+End - Toggle dimmer
    let toggle_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::End);
    
    // Ctrl+Alt+Up - Increase opacity
    let increase_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::ArrowUp);
    
    // Ctrl+Alt+Down - Decrease opacity  
    let decrease_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::ArrowDown);

    let app_handle = app.clone();
    app.global_shortcut().on_shortcut(toggle_shortcut, move |_app, shortcut, event| {
        // Only trigger on key press, not release
        if event.state != ShortcutState::Pressed {
            return;
        }
        println!("[DEBUG] Toggle shortcut PRESSED");
        if shortcut == &toggle_shortcut {
            do_toggle_dimmer(&app_handle);
        }
    })?;

    let app_handle = app.clone();
    app.global_shortcut().on_shortcut(increase_shortcut, move |_app, shortcut, event| {
        if event.state != ShortcutState::Pressed {
            return;
        }
        if shortcut == &increase_shortcut {
            do_adjust_opacity(&app_handle, 0.1);
        }
    })?;

    let app_handle = app.clone();
    app.global_shortcut().on_shortcut(decrease_shortcut, move |_app, shortcut, event| {
        if event.state != ShortcutState::Pressed {
            return;
        }
        if shortcut == &decrease_shortcut {
            do_adjust_opacity(&app_handle, -0.1);
        }
    })?;

    println!("[Shortcuts] Registered: Ctrl+Alt+End (toggle), Ctrl+Alt+Up/Down (adjust)");
    
    Ok(())
}

/// Build and run the Tauri application
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = load_config();
    let state = Arc::new(AppState {
        overlay_visible: Mutex::new(config.is_enabled),
        config: Mutex::new(config),
    });

    tauri::Builder::default()
        .manage(state.clone())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .setup(move |app| {
            // Build tray menu
            let toggle_i = MenuItem::with_id(app, "toggle", "Toggle Dimmer", true, None::<&str>)?;
            let settings_i = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

            let menu = Menu::with_items(app, &[&toggle_i, &settings_i, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "toggle" => {
                        let state: State<Arc<AppState>> = app.state();
                        let mut config = state.config.lock().unwrap();
                        config.is_enabled = !config.is_enabled;
                        save_config(&config);
                        let mut vis = state.overlay_visible.lock().unwrap();
                        *vis = config.is_enabled;
                        
                        // Show or hide overlay
                        #[cfg(target_os = "windows")]
                        {
                            if config.is_enabled {
                                overlay_impl::show_overlay(config.opacity, false); // Always exclude from capture
                            } else {
                                overlay_impl::hide_overlay();
                            }
                        }
                    }
                    "settings" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Show overlay on startup if enabled
            {
                let state: State<Arc<AppState>> = app.state();
                let config = state.config.lock().unwrap();
                if config.is_enabled {
                    // Force allow_capture=false to use WDA_EXCLUDEFROMCAPTURE
                    // This makes the overlay invisible to all screen capture tools
                    #[cfg(target_os = "windows")]
                    overlay_impl::show_overlay(config.opacity, false);
                }
            }

            // NOTE: Topmost monitor removed - it interfered with ShareX's UI
            // The overlay is set as TOPMOST when created, and WDA_EXCLUDEFROMCAPTURE
            // ensures screenshots don't capture it

            // Register global shortcuts
            register_shortcuts(app.handle())?;

            // NOTE: Keyboard hook disabled - using WDA_EXCLUDEFROMCAPTURE instead
            // The overlay is automatically hidden from screen captures
            // #[cfg(target_os = "windows")]
            // keyboard_hook::install_hook();

            // Show and focus main window on startup
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.set_focus();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_config, set_opacity, toggle_dimmer, set_allow_capture, set_auto_update, check_for_update])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

