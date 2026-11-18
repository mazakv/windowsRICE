//! Current session and shell state (read-only).

use serde::Serialize;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
use windows::core::{PCWSTR, w};

use crate::error::{WinError, api};
use crate::registry_read::{Hive, read_string};

const WINLOGON_KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon";

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    /// Whether this process runs with an elevated token.
    pub process_elevated: Option<bool>,
    /// HKLM Winlogon `Shell` (normally `explorer.exe`). Read only.
    pub winlogon_shell_machine: Option<String>,
    /// HKCU Winlogon `Shell` override, if any. Read only.
    pub winlogon_shell_user_override: Option<String>,
    /// Whether the Explorer taskbar window (`Shell_TrayWnd`) exists.
    pub explorer_taskbar_present: bool,
}

pub fn session_info() -> SessionInfo {
    SessionInfo {
        process_elevated: process_elevated()
            .inspect_err(|e| tracing::warn!(error = %e, "elevation query failed"))
            .ok(),
        winlogon_shell_machine: read_string(Hive::LocalMachine, WINLOGON_KEY, "Shell")
            .ok()
            .flatten(),
        winlogon_shell_user_override: read_string(Hive::CurrentUser, WINLOGON_KEY, "Shell")
            .ok()
            .flatten(),
        explorer_taskbar_present: explorer_taskbar_present(),
    }
}

pub fn process_elevated() -> Result<bool, WinError> {
    let mut token = HANDLE::default();
    // SAFETY: pseudo-handle for the current process; TOKEN_QUERY is read-only access.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) }
        .map_err(api("OpenProcessToken"))?;
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned: u32 = 0;
    // SAFETY: `elevation` is a correctly sized out buffer for TokenElevation.
    let result = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some((&raw mut elevation).cast()),
            u32::try_from(size_of::<TOKEN_ELEVATION>()).unwrap_or(0),
            &raw mut returned,
        )
    };
    // SAFETY: `token` was opened above and is closed exactly once.
    let _ = unsafe { CloseHandle(token) };
    result.map_err(api("GetTokenInformation"))?;
    Ok(elevation.TokenIsElevated != 0)
}

fn explorer_taskbar_present() -> bool {
    // SAFETY: lookup by class name only; no message is sent to the window.
    unsafe { FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) }.is_ok_and(|h| !h.is_invalid())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On-machine test.
    #[test]
    fn live_elevation_query_succeeds() {
        assert!(process_elevated().is_ok());
    }
}
