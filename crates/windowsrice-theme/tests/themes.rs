//! Theme parsing, preset loading and validation tests.

use std::collections::HashSet;

use windowsrice_theme::contrast::{palette_checks, translucent_surface_checks};
use windowsrice_theme::presets::{
    BUILTIN_PRESETS, DEFAULT_THEME_ID, default_theme, load_all_builtin, load_builtin,
};
use windowsrice_theme::schema::{AccentSource, Mode};
use windowsrice_theme::validate::{MAX_FONT_WEIGHT, Severity};
use windowsrice_theme::{MAX_THEME_FILE_BYTES, ThemeError, parse_theme};

fn default_source() -> &'static str {
    BUILTIN_PRESETS
        .iter()
        .find(|p| p.id == DEFAULT_THEME_ID)
        .unwrap()
        .source
}

/// Replaces exactly one occurrence, so a test never silently edits nothing.
fn replace_once(text: &str, from: &str, to: &str) -> String {
    assert_eq!(
        text.matches(from).count(),
        1,
        "pattern {from:?} must occur exactly once"
    );
    text.replacen(from, to, 1)
}

fn expect_invalid(text: &str, path: &str) {
    match parse_theme(text) {
        Err(ThemeError::Invalid(report)) => assert!(
            report.errors().any(|i| i.path == path),
            "expected an error at {path}, got:\n{report}"
        ),
        Err(other) => panic!("expected validation error at {path}, got {other}"),
        Ok(_) => panic!("expected validation error at {path}, theme was accepted"),
    }
}

fn expect_parse_error(text: &str, needle: &str) {
    match parse_theme(text) {
        Err(ThemeError::Parse(msg)) => assert!(
            msg.contains(needle),
            "parse error {msg:?} should mention {needle:?}"
        ),
        Err(other) => panic!("expected parse error, got {other}"),
        Ok(_) => panic!("expected parse error, theme was accepted"),
    }
}

// ---------------------------------------------------------------- presets

#[test]
fn all_eight_presets_load_without_errors() {
    let all = load_all_builtin();
    assert_eq!(all.len(), 8);
    for (preset, result) in &all {
        let loaded = result
            .as_ref()
            .unwrap_or_else(|e| panic!("{} failed:\n{e}", preset.file_name));
        assert_eq!(loaded.theme.meta.id, preset.id, "{}", preset.file_name);
        assert_eq!(format!("{}.toml", preset.id), preset.file_name);
    }
}

#[test]
fn preset_names_ids_and_order() {
    let names: Vec<String> = load_all_builtin()
        .into_iter()
        .map(|(_, r)| r.unwrap().theme.meta.name)
        .collect();
    assert_eq!(
        names,
        [
            "windowsRICE Default",
            "windowsRICE Minimal",
            "windowsRICE HYPR",
            "windowsRICE macOS Inspired",
            "windowsRICE Glass",
            "windowsRICE AMOLED",
            "windowsRICE Ambient",
            "windowsRICE Light",
        ]
    );
    let ids: HashSet<_> = BUILTIN_PRESETS.iter().map(|p| p.id).collect();
    assert_eq!(ids.len(), 8);
}

#[test]
fn all_presets_use_regular_typography_and_system_fonts() {
    for (preset, result) in load_all_builtin() {
        let t = result.unwrap().theme;
        let ty = &t.typography;
        assert_eq!(ty.weight_regular, 400, "{}", preset.id);
        assert_eq!(ty.weight_emphasis, 500, "{}", preset.id);
        assert!(ty.weight_strong <= MAX_FONT_WEIGHT, "{}", preset.id);
        assert_eq!(ty.family, "Segoe UI Variable", "{}", preset.id);
        assert!(
            ty.fallback.iter().any(|f| f == "Malgun Gothic"),
            "{}",
            preset.id
        );
        assert!(
            !preset.source.to_ascii_lowercase().contains("pretendard"),
            "{}",
            preset.id
        );
    }
}

#[test]
fn default_has_both_modes_and_light_preset_is_light() {
    let d = default_theme().theme;
    assert_eq!(d.meta.id, DEFAULT_THEME_ID);
    assert!(d.palette.dark.is_some() && d.palette.light.is_some());
    assert_eq!(d.meta.default_mode, Mode::Dark);

    let light = load_builtin("windowsrice-light").unwrap().unwrap().theme;
    assert_eq!(light.meta.default_mode, Mode::Light);
    assert!(light.palette.dark.is_none());
}

#[test]
fn every_palette_passes_every_contrast_rule() {
    for (preset, result) in load_all_builtin() {
        let t = result.unwrap().theme;
        for (mode, p) in t.palette.iter() {
            for c in palette_checks(p) {
                assert!(
                    c.passes,
                    "{} {mode:?}: {} on {} = {:.2}",
                    preset.id, c.foreground, c.background, c.ratio
                );
            }
        }
    }
}

#[test]
fn only_glass_reports_warnings_and_only_for_translucency() {
    let mut warned = Vec::new();
    for (preset, result) in load_all_builtin() {
        let loaded = result.unwrap();
        if loaded.report.warnings().next().is_some() {
            warned.push(preset.id);
            for w in loaded.report.warnings() {
                assert!(
                    w.path.starts_with("palette.") || w.path == "materials.surface_opacity",
                    "unexpected warning in {}: {} {}",
                    preset.id,
                    w.path,
                    w.message
                );
            }
        }
    }
    assert_eq!(warned, ["windowsrice-glass"]);
}

#[test]
fn ambient_requests_wallpaper_accent_with_theme_fallback() {
    let t = load_builtin("windowsrice-ambient").unwrap().unwrap().theme;
    assert_eq!(t.meta.accent_source, AccentSource::Wallpaper);
    assert!(t.palette.dark.is_some() && t.palette.light.is_some());
}

#[test]
fn themes_round_trip_through_toml() {
    for (preset, result) in load_all_builtin() {
        let t = result.unwrap().theme;
        let text = t.to_toml_string().unwrap();
        let again =
            parse_theme(&text).unwrap_or_else(|e| panic!("{} re-parse failed: {e}", preset.id));
        assert_eq!(again.theme, t, "{}", preset.id);
    }
}

// ---------------------------------------------------------- invalid input

#[test]
fn rejects_invalid_color_strings() {
    let base = default_source();
    for bad in ["#GGGGGG", "#FFF", "1B1D23", "red", "#1B1D2"] {
        let text = replace_once(
            base,
            "surface_base = \"#1B1D23\"",
            &format!("surface_base = \"{bad}\""),
        );
        expect_parse_error(&text, "invalid color");
    }
}

#[test]
fn rejects_missing_and_unknown_tokens() {
    let base = default_source();
    expect_parse_error(
        &replace_once(base, "text_muted = \"#9198A4\"\n", ""),
        "text_muted",
    );
    expect_parse_error(
        &replace_once(base, "padding = 16\n", "padding = 16\nglow = 3\n"),
        "glow",
    );
    expect_parse_error(
        &replace_once(base, "backdrop = \"acrylic\"", "backdrop = \"blur\""),
        "blur",
    );
    expect_parse_error(
        &replace_once(base, "weight_strong = 600", "weight_strong = \"bold\""),
        "weight_strong",
    );
}

#[test]
fn rejects_font_weights_above_600_and_non_regular_body() {
    let base = default_source();
    expect_invalid(
        &replace_once(base, "weight_strong = 600", "weight_strong = 700"),
        "typography.weight_strong",
    );
    expect_invalid(
        &replace_once(base, "weight_regular = 400", "weight_regular = 300"),
        "typography.weight_regular",
    );
    expect_invalid(
        &replace_once(base, "weight_emphasis = 500", "weight_emphasis = 600"),
        "typography.weight_emphasis",
    );
    expect_invalid(
        &replace_once(base, "weight_strong = 600", "weight_strong = 450"),
        "typography.weight_strong",
    );
}

#[test]
fn rejects_unsafe_typography_metrics() {
    let base = default_source();
    expect_invalid(
        &replace_once(base, "line_height = 1.45", "line_height = 1.1"),
        "typography.line_height",
    );
    expect_invalid(
        &replace_once(base, "display = 28", "display = 12"),
        "typography.sizes",
    );
    expect_invalid(
        &replace_once(base, "caption = 11", "caption = 8"),
        "typography.sizes.caption",
    );
}

#[test]
fn rejects_low_contrast_text_and_controls() {
    let base = default_source();
    expect_invalid(
        &replace_once(base, "text_muted = \"#9198A4\"", "text_muted = \"#4A4E57\""),
        "palette.dark.text_muted",
    );
    expect_invalid(
        &replace_once(
            base,
            "text_on_accent = \"#0E1116\"",
            "text_on_accent = \"#FFFFFF\"",
        ),
        "palette.dark.text_on_accent",
    );
    expect_invalid(
        &replace_once(
            base,
            "border_control = \"#FFFFFF61\"",
            "border_control = \"#FFFFFF20\"",
        ),
        "palette.dark.border_control",
    );
    expect_invalid(
        &replace_once(base, "focus_ring = \"#1F5FAE\"", "focus_ring = \"#DDE6F0\""),
        "palette.light.focus_ring",
    );
}

#[test]
fn rejects_translucent_surface_and_text_tokens() {
    let base = default_source();
    expect_invalid(
        &replace_once(
            base,
            "surface_raised = \"#23262E\"",
            "surface_raised = \"#23262ECC\"",
        ),
        "palette.dark.surface_raised",
    );
    expect_invalid(
        &replace_once(
            base,
            "text_primary = \"#1E2128\"",
            "text_primary = \"#1E212880\"",
        ),
        "palette.light.text_primary",
    );
}

#[test]
fn rejects_see_through_panels_without_backdrop() {
    let text = replace_once(
        default_source(),
        "backdrop = \"acrylic\"",
        "backdrop = \"none\"",
    );
    expect_invalid(&text, "materials.surface_opacity");
    let text = replace_once(
        default_source(),
        "surface_opacity = 0.88",
        "surface_opacity = 0.1",
    );
    expect_invalid(&text, "materials.surface_opacity");
}

#[test]
fn rejects_bad_metadata() {
    let base = default_source();
    expect_invalid(
        &replace_once(base, "schema_version = 1", "schema_version = 2"),
        "meta.schema_version",
    );
    expect_invalid(
        &replace_once(
            base,
            "id = \"windowsrice-default\"",
            "id = \"Windows Rice\"",
        ),
        "meta.id",
    );
    expect_invalid(
        &replace_once(base, "version = \"1.0.0\"", "version = \"1.0\""),
        "meta.version",
    );
    expect_invalid(
        &replace_once(base, "name = \"windowsRICE Default\"", "name = \"\""),
        "meta.name",
    );
}

#[test]
fn rejects_default_mode_without_palette() {
    let hypr = BUILTIN_PRESETS
        .iter()
        .find(|p| p.id == "windowsrice-hypr")
        .unwrap()
        .source;
    expect_invalid(
        &replace_once(hypr, "default_mode = \"dark\"", "default_mode = \"light\""),
        "meta.default_mode",
    );
}

#[test]
fn rejects_inconsistent_layout_and_motion() {
    let base = default_source();
    expect_invalid(
        &replace_once(base, "floating = true", "floating = false"),
        "components.bar.margin",
    );
    expect_invalid(&replace_once(base, "xl = 24", "xl = 10"), "spacing");
    expect_invalid(
        &replace_once(base, "duration_fast_ms = 120", "duration_fast_ms = 500"),
        "motion",
    );
    expect_invalid(
        &replace_once(
            base,
            "easing = [0.2, 0.0, 0.0, 1.0]",
            "easing = [1.5, 0.0, 0.0, 1.0]",
        ),
        "motion.easing",
    );
    expect_invalid(&replace_once(base, "dock = 16", "dock = 64"), "radius.dock");
    expect_invalid(
        &replace_once(
            base,
            "magnification_scale = 1.0",
            "magnification_scale = 3.0",
        ),
        "components.dock.magnification_scale",
    );
}

#[test]
fn missing_hangul_fallback_is_a_warning_not_an_error() {
    let text = replace_once(
        default_source(),
        "fallback = [\"Malgun Gothic\"]",
        "fallback = []",
    );
    let loaded = parse_theme(&text).expect("theme should still be valid");
    assert!(
        loaded
            .report
            .issues
            .iter()
            .any(|i| i.severity == Severity::Warning && i.path == "typography.fallback")
    );
}

#[test]
fn rejects_oversized_input() {
    let huge = format!(
        "{}\n#{}",
        default_source(),
        "x".repeat(MAX_THEME_FILE_BYTES)
    );
    assert!(matches!(
        parse_theme(&huge),
        Err(ThemeError::TooLarge { .. })
    ));
}

#[test]
fn rejects_non_toml_input() {
    assert!(matches!(
        parse_theme("not = [valid"),
        Err(ThemeError::Parse(_))
    ));
    assert!(matches!(parse_theme(""), Err(ThemeError::Parse(_))));
}

// --------------------------------------------------------------- reporting

/// Prints the contrast table used in the Phase 2 report
/// (`cargo test -p windowsrice-theme --test themes contrast_table -- --nocapture`).
#[test]
fn contrast_table() {
    for (preset, result) in load_all_builtin() {
        let t = result.unwrap().theme;
        for (mode, p) in t.palette.iter() {
            let checks = palette_checks(p);
            let min_text = checks
                .iter()
                .filter(|c| c.required > 4.0)
                .map(|c| c.ratio)
                .fold(f64::MAX, f64::min);
            let min_non_text = checks
                .iter()
                .filter(|c| c.required < 4.0)
                .map(|c| c.ratio)
                .fold(f64::MAX, f64::min);
            let weakest = checks
                .iter()
                .min_by(|a, b| (a.ratio / a.required).total_cmp(&(b.ratio / b.required)))
                .unwrap();
            let translucent_min =
                translucent_surface_checks(p, f64::from(t.materials.surface_opacity))
                    .iter()
                    .map(|c| c.ratio)
                    .fold(f64::NAN, f64::min);
            println!(
                "{:<28} {:<5} checks={:>2} min_text={:>5.2} min_non_text={:>5.2} weakest={}/{}={:.2} worst_translucent={}",
                preset.id,
                format!("{mode:?}").to_lowercase(),
                checks.len(),
                min_text,
                min_non_text,
                weakest.foreground,
                weakest.background,
                weakest.ratio,
                if translucent_min.is_nan() {
                    "n/a (opaque)".into()
                } else {
                    format!("{translucent_min:.2}")
                }
            );
        }
    }
}
