//! Schema v3 (`[bar]`) and migration tests.

use std::path::Path;

use windowsrice_core::bar::{BarConfig, ModuleKind, MonitorPlacement, Region};
use windowsrice_core::config::{CONFIG_SCHEMA_VERSION, Config, LoadOutcome};

fn origin() -> &'static Path {
    Path::new("config.toml")
}

#[test]
fn current_version_is_3() {
    assert_eq!(CONFIG_SCHEMA_VERSION, 3);
    assert_eq!(Config::default().schema_version, 3);
}

#[test]
fn v2_file_migrates_and_keeps_every_setting() {
    let v2 = r#"
schema_version = 2
[general]
language = "korean"
[theme]
active = "windowsrice-glass"
mode = "light"
[typography]
text_scale = 1.25
[launcher]
hotkey = "Alt+Shift+K"
"#;
    let (c, m) = Config::from_toml_str_migrating(v2, origin()).unwrap();
    let m = m.expect("migration expected");
    assert_eq!(m.from_version, 2);
    assert!(m.notes.iter().any(|n| n.contains("[bar]")));
    assert_eq!(c.schema_version, 3);
    assert_eq!(c.theme.active, "windowsrice-glass");
    assert_eq!(c.typography.text_scale, 1.25);
    assert_eq!(c.launcher.hotkey.to_string(), "Alt+Shift+K");
    assert_eq!(c.bar, BarConfig::default());
}

#[test]
fn v1_file_migrates_through_v2_to_v3() {
    let v1 = "schema_version = 1\n[general]\nactive_theme = \"windowsRICE Minimal\"\n";
    let (c, m) = Config::from_toml_str_migrating(v1, origin()).unwrap();
    let m = m.unwrap();
    assert_eq!(m.from_version, 1);
    assert_eq!(m.notes.len(), 2, "{:?}", m.notes);
    assert_eq!(c.theme.active, "windowsrice-minimal");
    assert_eq!(c.bar, BarConfig::default());
}

#[test]
fn v2_with_bar_section_is_rejected() {
    let bad = "schema_version = 2\n[bar]\nspacing = 2\n";
    let e = Config::from_toml_str(bad, origin())
        .unwrap_err()
        .to_string();
    assert!(e.contains("[bar]"), "{e}");
}

#[test]
fn v3_round_trips_custom_bar() {
    let mut c = Config::default();
    c.bar.height = Some(40);
    c.bar.auto_hide = true;
    c.bar.monitors = MonitorPlacement::Primary;
    c.bar.modules.retain(|m| m.kind != ModuleKind::Battery);
    let cpu = c.bar.modules.iter_mut().find(|m| m.id == "cpu").unwrap();
    cpu.interval_ms = Some(1000);
    cpu.region = Region::Left;
    let text = c.bar.modules.iter_mut().find(|m| m.id == "text").unwrap();
    text.enabled = true;
    text.text = Some("안녕하세요".into());
    let toml = c.to_toml_string().unwrap();
    let (back, migration) = Config::from_toml_str_migrating(&toml, origin()).unwrap();
    assert!(migration.is_none());
    assert_eq!(back, c);
}

#[test]
fn invalid_bar_fails_with_a_useful_message() {
    let text = r#"
schema_version = 3
[bar]
height = 200
[[bar.modules]]
id = "clock"
kind = "clock"
"#;
    let e = Config::from_toml_str(text, origin())
        .unwrap_err()
        .to_string();
    assert!(e.contains("bar.height = 200"), "{e}");
    assert!(e.contains("menu module"), "{e}");
}

#[test]
fn unknown_bar_keys_are_rejected_not_dropped() {
    let text = "schema_version = 3\n[bar]\nmystery = 1\n";
    let e = Config::from_toml_str(text, origin())
        .unwrap_err()
        .to_string();
    assert!(e.contains("mystery"), "{e}");
}

#[test]
fn invalid_file_is_left_untouched_and_reported() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let original = "schema_version = 3\n[bar]\nspacing = 99\n";
    std::fs::write(&path, original).unwrap();
    let (c, outcome) = Config::load_or_default(&path);
    assert_eq!(c, Config::default());
    match outcome {
        LoadOutcome::Invalid { error } => assert!(error.contains("bar.spacing"), "{error}"),
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
}

#[test]
fn future_version_is_refused() {
    let e = Config::from_toml_str("schema_version = 4\n", origin())
        .unwrap_err()
        .to_string();
    assert!(e.contains("newer"), "{e}");
}

#[test]
fn design_studio_fields_are_optional_and_validated() {
    // A Phase 5 v3 file without the new fields still loads.
    let phase5 = "schema_version = 3\n[bar]\nspacing = 6\n";
    let c = Config::from_toml_str(phase5, origin()).unwrap();
    assert_eq!(c.bar.style, windowsrice_core::bar::BarStyle::default());
    // New fields round-trip and are validated.
    let mut c = Config::default();
    c.bar.style.background = Some("#101418".into());
    c.bar.style.opacity = Some(0.8);
    c.bar.style.font_weight = Some(500);
    let cpu = c.bar.modules.iter_mut().find(|m| m.id == "cpu").unwrap();
    cpu.show_label = Some(false);
    cpu.max_width = Some(120);
    let t = c.to_toml_string().unwrap();
    assert_eq!(Config::from_toml_str(&t, origin()).unwrap(), c);
    let mut bad = Config::default();
    bad.bar.style.background = Some("blue".into());
    bad.bar.style.font_weight = Some(700);
    bad.bar.modules[0].hour12 = Some(true);
    let e = bad.validate().unwrap_err().to_string();
    assert!(
        e.contains("#RRGGBB") && e.contains("outside 300-600") && e.contains("hour12"),
        "{e}"
    );
}

#[test]
fn phase8_design_fields_round_trip_and_keep_old_files_unchanged() {
    use windowsrice_core::bar::{
        AccentLine, Density, Gradient, GradientDirection, IconSet, IconTint, Material,
        ModuleContent, ModuleShape,
    };
    // Defaults add nothing to the written file.
    let before = Config::default().to_toml_string().unwrap();
    for key in [
        "material",
        "gradient",
        "module_shape",
        "icon_set",
        "density",
    ] {
        assert!(
            !before.contains(key),
            "{key} written while default:\n{before}"
        );
    }
    let mut c = Config::default();
    let s = &mut c.bar.style;
    s.material = Material::Translucent;
    s.gradient = Some(Gradient {
        direction: GradientDirection::Diagonal,
        colors: vec!["#101418".into(), "#2A3140".into(), "#3A8DDE".into()],
    });
    s.module_background = Some("#202830".into());
    s.clock_color = Some("#E3A33B".into());
    s.module_shape = ModuleShape::Pill;
    s.module_opacity = Some(60);
    s.content = ModuleContent::IconOnly;
    s.density = Density::Compact;
    s.icon_set = IconSet::Classic;
    s.icon_tint = IconTint::Colorful;
    s.icon_size = Some(2);
    s.accent_line = AccentLine::Bottom;
    s.shadow = Some(40);
    s.region_gap = Some(32);
    s.clock_size = Some(16);
    s.font_weight = Some(300);
    c.validate().unwrap();
    let t = c.to_toml_string().unwrap();
    assert_eq!(Config::from_toml_str(&t, origin()).unwrap(), c);

    let mut bad = Config::default();
    bad.bar.style.gradient = Some(Gradient {
        direction: GradientDirection::Vertical,
        colors: vec!["#101418".into()],
    });
    bad.bar.style.module_opacity = Some(5);
    bad.bar.style.clock_color = Some("red".into());
    let e = bad.validate().unwrap_err().to_string();
    assert!(
        e.contains("2-4 colors") && e.contains("module_opacity") && e.contains("clock_color"),
        "{e}"
    );
}
