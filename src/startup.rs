//! "Start with Windows": a per-user Run key entry that launches the app
//! hidden in the tray, so the AI key keeps working after a restart.
//! HKCU only, so no admin rights are needed.

use std::ptr::null_mut;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegDeleteValueW, RegOpenKeyExW,
    RegSetValueExW,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "RightCtrlBack";

/// Command-line flag: start with the window hidden in the tray.
pub const TRAY_ARG: &str = "--tray";

/// The Run value data for an exe path.
pub fn command_for(exe: &std::path::Path) -> String {
    format!("\"{}\" {}", exe.display(), TRAY_ARG)
}

/// Whether the app should start hidden, from its command-line arguments.
pub fn wants_tray(args: &[String]) -> bool {
    args.iter().any(|a| a == TRAY_ARG)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Add or remove the Run entry. Writing it every launch also repairs the path
/// if the app has moved.
pub fn apply(enabled: bool) {
    unsafe {
        let mut key: HKEY = null_mut();
        let path = wide(RUN_KEY);
        if RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_SET_VALUE, &mut key) != ERROR_SUCCESS {
            return;
        }
        let name = wide(VALUE_NAME);
        if enabled {
            if let Ok(exe) = std::env::current_exe() {
                let data = wide(&command_for(&exe));
                RegSetValueExW(
                    key,
                    name.as_ptr(),
                    0,
                    REG_SZ,
                    data.as_ptr() as *const u8,
                    (data.len() * 2) as u32,
                );
            }
        } else {
            RegDeleteValueW(key, name.as_ptr());
        }
        RegCloseKey(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_quotes_path_and_adds_tray_flag() {
        let cmd = command_for(std::path::Path::new(r"C:\Users\A B\app.exe"));
        assert_eq!(cmd, r#""C:\Users\A B\app.exe" --tray"#);
    }

    #[test]
    fn tray_flag_detection() {
        assert!(wants_tray(&["--tray".to_string()]));
        assert!(!wants_tray(&[]));
        assert!(!wants_tray(&["--auto-update".to_string()]));
    }
}
