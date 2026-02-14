# Changelog

## v1.1.0
- Fix overlay flicker during screenshots by handling `WM_WINDOWPOSCHANGING` instead of polling `SetWindowPos`.
- Improve ShareX/PrintScreen reliability (no focus hacks needed).
- Add in-app auto-update toggle, check button, and graceful “no update” handling when no releases exist.
- Add GitHub Actions release workflow and updater configuration pointing to KDSPL/savemyeyes.
- Portable `.exe` build confirmed; `.msi` installer remains available.

## v1.0.0
- Initial Windows release: multi-monitor dimming overlay, capture-safe via `WDA_EXCLUDEFROMCAPTURE`, global hotkeys, tray menu, autostart, config persistence, and modern UI.
