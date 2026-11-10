//! Self-update via GitHub Releases, using Velopack. Factory-owned: keep this
//! file as-is unless the app needs install/uninstall hooks of its own.
//!
//! Only works on a build installed by Velopack - a `cargo run` dev build has
//! no install root for `UpdateManager` to find, and every function here
//! quietly does nothing in that case.

use velopack::sources::GithubSource;
use velopack::{UpdateCheck, UpdateInfo, UpdateManager, VelopackApp};

const REPO_URL: &str = env!("CARGO_PKG_REPOSITORY");
const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;

/// Must run before anything else in `main` - Velopack may terminate or
/// restart the process to carry out an install/uninstall/update step.
pub fn run_app_hooks() {
    VelopackApp::build().run();
}

/// The running version, for display.
pub fn current_version() -> String {
    manager()
        .map(|um| um.get_current_version_as_string())
        .unwrap_or_else(|_| format!("{} (dev)", env!("CARGO_PKG_VERSION")))
}

fn manager() -> Result<UpdateManager, String> {
    let source = GithubSource::new(REPO_URL, None, false);
    UpdateManager::new(source, None, None).map_err(|e| e.to_string())
}

fn check() -> Option<UpdateInfo> {
    let um = manager().ok()?;
    match um.check_for_updates().ok()? {
        UpdateCheck::UpdateAvailable(info) => Some(*info),
        _ => None,
    }
}

/// Called at startup. If auto-update is on and the last check was over a day
/// ago, stamps the time and spawns a detached `--auto-update` child, so a
/// download never holds up (or dies with) the main window.
pub fn maybe_spawn_background_update() {
    let mut settings = crate::settings::load();
    if !settings.auto_update {
        return;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if now.saturating_sub(settings.last_update_check) < CHECK_INTERVAL_SECS {
        return;
    }
    settings.last_update_check = now;
    crate::settings::save(&settings);

    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe)
            .arg("--auto-update")
            .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
            .spawn();
    }
}

/// Body of the `--auto-update` child: check, download, and hand the apply
/// step to Velopack, which waits for this process to exit. The new version
/// runs on the next launch.
pub fn run_background_update() {
    let Some(info) = check() else { return };
    let Ok(um) = manager() else { return };
    if um.download_updates(&info, None).is_err() {
        return;
    }
    let _ = um.wait_exit_then_apply_updates(&info.TargetFullRelease, true, false, [] as [&str; 0]);
}
