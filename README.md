# Right Ctrl Back

Turn the AI key on new laptops back into Right Ctrl, or anything else you choose.

Most laptops sold since 2024 replaced Right Ctrl with an AI key that sends the chord
Win+Shift+F23. Right Ctrl Back listens for that chord, blocks it, and sends the key you
prefer instead. As Right Ctrl it is a true held modifier, so it works with Shift and Alt
combinations and with other keys.

## Features

- Recognises the Win+Shift+F23 chord with a low-level keyboard hook and suppresses it
- Maps it to a held Right Ctrl, the menu key, a media key, any single key, or launches an app
- Key tester showing what the key sends and what the app sends instead
- Starts with Windows, hidden in the tray, so the key is remapped after every restart (can be switched off)
- Tray icon with Pause / Resume, Open and Quit; closing the window keeps it running
- Keyboard shortcuts: Ctrl+P pause or resume, Ctrl+Q quit

## Install

- **Microsoft Store:** search for "Right Ctrl Back".
- **GitHub Releases:** download the installer from
  <https://github.com/fredle/right-ctrl-back/releases>. It installs per user, with no
  administrator rights, and updates itself.

## Limitations

- Windows does not let a normal app see or send keys in windows running as administrator, so
  the AI key is not remapped while an elevated window has focus. This app never asks for
  elevation.
- Only the AI key is remapped. It is not a general remapper, it does not use keyboard
  scancode maps, and it has no macros.

## Data

Settings (your chosen target and the auto-update choice) are stored in
`%LOCALAPPDATA%\RightCtrlBack\settings.json`. While "Start with Windows" is on, a
per-user `RightCtrlBack` value is kept under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`; switching the option off or
uninstalling removes it. Nothing else is written.

## Privacy

- No telemetry, no accounts.
- The only network call is the update check against GitHub Releases, about once a day,
  which can be switched off with the "Automatically update" checkbox.
- The key tester lists keys only while it is switched on, in memory only, and is cleared
  when you stop it or close the window.

## Build

```
cargo build --release
cargo test
```

Rust 2024, egui/eframe, Velopack for install and updates.
