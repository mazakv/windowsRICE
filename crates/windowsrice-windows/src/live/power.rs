//! Battery and notification state (read-only).

use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::UI::Shell::{
    QUNS_ACCEPTS_NOTIFICATIONS, QUNS_APP, QUNS_BUSY, QUNS_NOT_PRESENT, QUNS_PRESENTATION_MODE,
    QUNS_QUIET_TIME, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState,
};

use crate::error::{WinError, api};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Battery {
    pub percent: Option<u8>,
    pub charging: bool,
    pub on_ac: bool,
}

const BATTERY_FLAG_CHARGING: u8 = 8;
const BATTERY_FLAG_NO_BATTERY: u8 = 128;
const BATTERY_FLAG_UNKNOWN: u8 = 255;

/// The system battery, or `None` when the computer has none.
pub fn battery() -> Result<Option<Battery>, WinError> {
    let mut s = SYSTEM_POWER_STATUS::default();
    // SAFETY: out-parameter to a local struct.
    unsafe { GetSystemPowerStatus(&raw mut s) }.map_err(api("GetSystemPowerStatus"))?;
    if s.BatteryFlag == BATTERY_FLAG_NO_BATTERY {
        return Ok(None);
    }
    if s.BatteryFlag == BATTERY_FLAG_UNKNOWN {
        return Err(WinError::Unavailable("battery status unknown".into()));
    }
    Ok(Some(Battery {
        percent: (s.BatteryLifePercent <= 100).then_some(s.BatteryLifePercent),
        charging: s.BatteryFlag & BATTERY_FLAG_CHARGING != 0,
        on_ac: s.ACLineStatus == 1,
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationState {
    Accepting,
    /// Windows reports quiet time.
    QuietTime,
    /// A fullscreen, presentation or busy application suppresses
    /// notifications.
    Suppressed,
}

/// Windows' user-notification state (`SHQueryUserNotificationState`). It
/// does not expose the number of notifications.
pub fn notification_state() -> Result<NotificationState, WinError> {
    // SAFETY: no preconditions.
    let s =
        unsafe { SHQueryUserNotificationState() }.map_err(api("SHQueryUserNotificationState"))?;
    match s {
        QUNS_ACCEPTS_NOTIFICATIONS => Ok(NotificationState::Accepting),
        QUNS_QUIET_TIME => Ok(NotificationState::QuietTime),
        QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE | QUNS_APP => {
            Ok(NotificationState::Suppressed)
        }
        QUNS_NOT_PRESENT => Err(WinError::Unavailable(
            "no interactive notification session".into(),
        )),
        other => Err(WinError::Unavailable(format!(
            "unknown notification state {}",
            other.0
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real Windows: both queries answer (a desktop without a battery
    /// returns `None`).
    #[test]
    fn queries_answer() {
        if let Some(b) = battery().unwrap() {
            assert!(b.percent.is_none_or(|p| p <= 100));
        }
        let _ = notification_state();
    }
}
