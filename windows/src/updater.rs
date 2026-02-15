// Windows-specific update logic, delegates to shared crate for core check

// Re-export shared constants and types
pub use savemyeyes_shared::updater::APP_VERSION;
pub use savemyeyes_shared::updater::UpdateResult;

/// Check for updates (looks for .exe assets)
pub fn check_for_update(current_version: &str) -> UpdateResult {
    savemyeyes_shared::updater::check_for_update(current_version, ".exe")
}

/// Open a URL in the default browser (Win32 ShellExecuteW)
pub fn open_url(url: &str) {
    let url_wide: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
    let verb: Vec<u16> = "open\0".encode_utf16().collect();
    unsafe {
        windows::Win32::UI::Shell::ShellExecuteW(
            None,
            windows::core::PCWSTR(verb.as_ptr()),
            windows::core::PCWSTR(url_wide.as_ptr()),
            None,
            None,
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        );
    }
}

/// Download the update .exe to a temp file.
pub fn download_update(download_url: &str) -> Result<std::path::PathBuf, String> {
    savemyeyes_shared::updater::download_to_temp(download_url, "savemyeyes_update.exe")
}

/// Replace the current exe with the downloaded update and relaunch.
pub fn apply_update_and_relaunch(downloaded_path: &std::path::Path) -> Result<(), String> {
    let current_exe = std::env::current_exe()
        .map_err(|e| format!("Cannot determine current exe path: {}", e))?;

    let old_path = current_exe.with_extension("exe.old");

    let _ = std::fs::remove_file(&old_path);

    std::fs::rename(&current_exe, &old_path)
        .map_err(|e| format!("Failed to rename current exe: {}", e))?;

    std::fs::copy(downloaded_path, &current_exe)
        .map_err(|e| {
            let _ = std::fs::rename(&old_path, &current_exe);
            format!("Failed to copy new exe: {}", e)
        })?;

    let _ = std::fs::remove_file(downloaded_path);

    let _ = std::process::Command::new(&current_exe)
        .arg("--updated")
        .spawn();

    std::process::exit(0);
}

/// Check if the app was just updated (launched with --updated flag)
pub fn was_just_updated() -> bool {
    savemyeyes_shared::updater::was_just_updated()
}

/// Clean up the .old file from a previous update
pub fn cleanup_old_exe() {
    if let Ok(current) = std::env::current_exe() {
        let old_path = current.with_extension("exe.old");
        let _ = std::fs::remove_file(old_path);
    }
}

/// Show a Win32 Yes/No message box asking the user to update.
pub fn prompt_update_dialog(new_version: &str) -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, IDYES, MB_ICONINFORMATION, MB_YESNO};

    let message = format!(
        "A new version (v{}) of SaveMyEyes is available!\n\nWould you like to download and install it now?\n\nThe app will restart automatically after updating.",
        new_version
    );
    let msg_wide: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
    let title_wide: Vec<u16> = "SaveMyEyes Update".encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let result = MessageBoxW(
            None,
            PCWSTR(msg_wide.as_ptr()),
            PCWSTR(title_wide.as_ptr()),
            MB_YESNO | MB_ICONINFORMATION,
        );
        result == IDYES
    }
}
