# SaveMyEyes

<p align="center">
  <strong>An open-source screen dimmer by <a href="https://kraftpixel.com">KraftPixel</a></strong>
</p>

A lightweight desktop utility that reduces screen luminance via a software overlay for visual comfort. The overlay is automatically hidden from screenshots and screen recordings.

**Quick download:** grab the latest `.exe` (portable) or `.msi` (installer) from the Releases page.

## Features

- 🌙 **Adjustable Dimming** - Reduce screen brightness from 0% to 90%
- 🖥️ **Multi-Monitor Support** - Covers all connected displays
- 📸 **Capture-Safe** - Automatically hidden from screenshots and recordings
- ⌨️ **Global Hotkeys** - Control from anywhere
- 🚀 **Lightweight** - Near-zero CPU usage, <50MB RAM
- 🎨 **Modern UI** - Clean, dark theme interface

## Hotkeys

| Action | Shortcut |
|--------|----------|
| Toggle On/Off | `Ctrl + Alt + End` |
| Increase Opacity | `Ctrl + Alt + Up` |
| Decrease Opacity | `Ctrl + Alt + Down` |

## Installation

### Windows
Download the latest `.msi` or `.exe` installer from the [Releases](https://github.com/KDSPL/savemyeyes/releases) page. The `.exe` build is portable (no install required); the `.msi` is the installer.

### Build from Source
```bash
# Clone the repository
git clone https://github.com/KDSPL/savemyeyes.git
cd savemyeyes

# Install dependencies
npm install

# Run in development
npm run tauri dev

# Build for production
npm run tauri build
```

## Screenshots

Place these files in `src/assets/` (PNG or JPG). 1280×720 or any clear 16:9 works well.

- Dimmer tab: `src/assets/screenshot-dimmer.png`

  ![Dimmer Tab](src/assets/screenshot-dimmer.png)

- Settings tab: `src/assets/screenshot-settings.png`

  ![Settings Tab](src/assets/screenshot-settings.png)

- Shortcuts tab: `src/assets/screenshot-shortcuts.png`

  ![Shortcuts Tab](src/assets/screenshot-shortcuts.png)

## Tech Stack

- **Framework:** [Tauri v2](https://tauri.app) 
- **Backend:** Rust
- **Frontend:** TypeScript, HTML, CSS
- **Windows APIs:** `windows-rs` for Win32 overlay windows

## Configuration

Settings are stored in `%AppData%\SaveMyEyes\config.json`:
- Opacity level
- Enabled state
- Autostart preference
- Hotkey bindings

## Screenshots

*The dimmer overlay doesn't appear in screenshots because that's the whole point!* 😄

## License

MIT License - see [LICENSE](LICENSE) for details.

## Acknowledgments

SaveMyEyes is built with these amazing open-source projects:

- [Tauri](https://tauri.app) - Desktop app framework
- [Rust](https://www.rust-lang.org) - Systems programming language
- [windows-rs](https://github.com/microsoft/windows-rs) - Rust bindings for Windows APIs
- [Vite](https://vitejs.dev) - Frontend build tool

## Credits

Built with ❤️ by [KraftPixel](https://kraftpixel.com)
