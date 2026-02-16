// macOS-specific update logic, delegates to shared crate

pub use savemyeyes_shared::updater::APP_VERSION;
pub use savemyeyes_shared::updater::UpdateResult;

/// Check for updates (looks for .dmg assets)
pub fn check_for_update(current_version: &str) -> UpdateResult {
    savemyeyes_shared::updater::check_for_update(current_version, ".dmg")
}

/// Open a URL in the default browser
pub fn open_url(url: &str) {
    savemyeyes_shared::updater::open_url(url);
}

/// Download the update .dmg to a temp file.
#[allow(dead_code)]
pub fn download_update(download_url: &str) -> Result<std::path::PathBuf, String> {
    savemyeyes_shared::updater::download_to_temp(download_url, "SaveMyEyes_update.dmg")
}

/// Check if the app was just updated
#[allow(dead_code)]
pub fn was_just_updated() -> bool {
    savemyeyes_shared::updater::was_just_updated()
}
