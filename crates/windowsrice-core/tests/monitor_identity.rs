//! Phase 10: per-monitor settings when Windows does or does not report a
//! stable monitor identity (device path). Pure tests of the matching rules.

use windowsrice_core::bar::{BarConfig, MonitorOverride};

fn ov(id: Option<&str>, device: &str, enabled: bool, height: Option<u16>) -> MonitorOverride {
    MonitorOverride {
        id: id.map(str::to_owned),
        device: device.to_owned(),
        enabled,
        height,
    }
}

fn with_overrides(monitor_overrides: Vec<MonitorOverride>) -> BarConfig {
    BarConfig {
        monitor_overrides,
        ..Default::default()
    }
}

const A: &str = r"\?\DISPLAY#MSI3CB5#5&a&0&UID256#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}";
const B: &str = r"\?\DISPLAY#DEL4321#5&b&0&UID512#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}";

#[test]
fn identified_monitor_matches_by_id_even_after_renumbering() {
    let b = with_overrides(vec![ov(Some(A), r"\.\DISPLAY2", false, Some(40))]);
    // Windows renumbered the GDI name; the id still matches.
    assert!(!b.monitor_enabled_among(r"\.\DISPLAY7", Some(A), true, &[Some(A)]));
    assert_eq!(
        b.height_for_among(r"\.\DISPLAY7", Some(A), &[Some(A)]),
        Some(40)
    );
}

#[test]
fn unidentified_monitor_keeps_its_setting_by_device_name() {
    // The setting was saved with an id; now Windows reports none (sleeping
    // DisplayPort monitor). The setting is not lost.
    let b = with_overrides(vec![ov(Some(A), r"\.\DISPLAY1", false, Some(44))]);
    assert!(!b.monitor_enabled_among(r"\.\DISPLAY1", None, true, &[None]));
    assert_eq!(b.height_for_among(r"\.\DISPLAY1", None, &[None]), Some(44));
}

#[test]
fn settings_of_another_connected_monitor_are_never_borrowed() {
    // Monitor B now sits at the GDI name that monitor A had when its
    // setting was saved, while A is connected elsewhere: B must not take
    // A's setting.
    let b = with_overrides(vec![ov(Some(A), r"\.\DISPLAY1", false, Some(44))]);
    let connected = [Some(A), Some(B)];
    assert!(b.monitor_enabled_among(r"\.\DISPLAY1", Some(B), false, &connected));
    assert_eq!(
        b.height_for_among(r"\.\DISPLAY1", Some(B), &connected),
        None
    );
    // A, at its new name, still has its own setting.
    assert!(!b.monitor_enabled_among(r"\.\DISPLAY3", Some(A), true, &connected));
    // An unidentified monitor at A's old name with A connected elsewhere:
    // no borrowing either.
    assert!(b.monitor_enabled_among(r"\.\DISPLAY1", None, false, &[Some(A), None]));
}

#[test]
fn legacy_settings_without_id_still_apply() {
    let b = with_overrides(vec![ov(None, r"\.\DISPLAY2", false, None)]);
    assert!(!b.monitor_enabled_among(r"\.\DISPLAY2", Some(B), false, &[Some(B)]));
    assert!(!b.monitor_enabled_among(r"\.\DISPLAY2", None, false, &[None]));
    assert!(b.monitor_enabled_among(r"\.\DISPLAY3", None, false, &[None]));
}

#[test]
fn exact_id_wins_over_device_name() {
    let b = with_overrides(vec![
        ov(None, r"\.\DISPLAY1", false, None),
        ov(Some(A), r"\.\DISPLAY9", true, Some(30)),
    ]);
    assert!(b.monitor_enabled_among(r"\.\DISPLAY1", Some(A), true, &[Some(A)]));
    assert_eq!(
        b.height_for_among(r"\.\DISPLAY1", Some(A), &[Some(A)]),
        Some(30)
    );
}

#[test]
fn a_different_identified_monitor_never_takes_a_setting() {
    // Monitor A's setting was saved at DISPLAY3; now a different,
    // identified monitor B is at DISPLAY3 and A is gone.
    let b = with_overrides(vec![ov(Some(A), r"\.\DISPLAY3", false, Some(44))]);
    assert!(b.monitor_enabled_among(r"\.\DISPLAY3", Some(B), true, &[Some(B)]));
    assert_eq!(
        b.height_for_among(r"\.\DISPLAY3", Some(B), &[Some(B)]),
        None
    );
}
