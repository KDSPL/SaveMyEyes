# Changelog

## v0.9.4
- **macOS native port** — Full AppKit-based macOS app via objc2 with card-based dark UI, NSSlider controls, and custom toggle switches.
- **Per-display brightness memory** — Brightness settings are now persisted by display name so reconnecting a monitor restores its last brightness level.
- **Actual display names** — Monitor sliders show real display names (e.g. "Built-in Retina Display") instead of generic "Monitor 1, Monitor 2" labels; long names are truncated.
- **Cursor-aware hotkeys** — Increase/Decrease hotkeys target the monitor under the mouse cursor.
- **Capture-safe overlays (macOS)** — Dimming overlays are hidden from screenshots and screen recordings.
- **Multi-monitor independent dimming** — Each connected display can be dimmed independently with its own slider.
- **Smart auto-updater** — Downloads the update .dmg, gracefully quits the running app, replaces the bundle, and relaunches automatically.
- **Monitor hot-plug support** — Detects monitor connect/disconnect events, refreshes overlays and rebuilds the settings UI automatically.
- **Accessibility permission indicator** — Shortcuts tab shows green/red status for Accessibility permission.
- Keyboard shortcut badges split into individual key pills with "+" separators, vertically centered.
- Improved badge text centering in percentage pills.
- Vertically centered text and toggles inside all card sections.
- Updated README with cross-platform documentation, macOS hotkeys, and build instructions.
- GitHub Actions release workflow updated for both Windows and macOS builds.

## v0.9.2
- Fix auto-updater issue

## v0.9.1
- Removed dependency on Tauri to reduce memory footprint (now at around 5 MB).
- Changed the dimming approach from using a Window to using the Windows Magnification API.
- Release now produces a single .exe file. Installer on the todo list.

## v0.9.0
- Added updater integration with in-app update toggle and manual check flow.
- Improved overlay stability and reduced flicker during screenshot / recording workflows.
- Added draft release automation for Windows artifacts (`.exe` and `.msi`).

## v0.8.0
- Improved keyboard shortcut handling and tray interactions.
- Refined settings behavior and config persistence on app restart.
- Improved multi-monitor overlay consistency.

## v0.7.0
- Introduced auto-start preference handling.
- Improved capture-safe mode behavior for common screen capture tools.
- UI polish for controls and status messaging.

## v0.6.0
- Expanded global hotkey support and toggle responsiveness.
- Improved dimmer opacity adjustment logic.
- Internal stability improvements for overlay lifecycle.

## v0.5.0
- Added system tray controls for quick enable/disable and exit.
- Improved startup flow and reduced friction on first run.
- Improved error handling around native window setup.

## v0.4.0
- Added persistent app configuration storage.
- Improved Windows overlay behavior across multiple displays.
- Refined app window behavior and hidden startup mode.

## v0.3.0
- Added modernized UI structure for dimmer/settings workflows.
- Added initial settings panel and status feedback patterns.
- Improved front-end to backend command wiring.

## v0.2.0
- Added initial global hotkey integration.
- Added initial tray and background behavior.
- Improved project structure for Tauri + TypeScript app flow.

## v0.1.0
- Initial app prototype with basic screen dimming overlay.
- Initial Tauri desktop setup and Windows-focused baseline implementation.
