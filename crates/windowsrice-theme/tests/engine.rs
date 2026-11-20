//! Theme engine tests: catalog loading, activation, rollback, fallback and
//! configuration persistence. All file-system tests run in temporary folders.

/// Built-in themes shipped with windowsRICE (8 until Phase 8, 17 since).
const BUILTIN_COUNT: usize = 17;

use std::path::{Path, PathBuf};

use windowsrice_core::config::{
    Config, DEFAULT_THEME_ID as CORE_DEFAULT_ID, ThemeModePreference, ThemeSelection,
};
use windowsrice_theme::catalog::{IssueKind, ThemeCatalog, ThemeSource};
use windowsrice_theme::engine::{
    ActivationError, ActivationStatus, NoopSink, ResolvedTheme, ThemeEngine, ThemeSink,
    resolve_mode,
};
use windowsrice_theme::notice::{Notice, NoticeLevel};
use windowsrice_theme::presets::{BUILTIN_PRESETS, DEFAULT_THEME_ID};
use windowsrice_theme::service::{SelectError, ThemeService};
use windowsrice_theme::{MAX_THEME_FILE_BYTES, Mode, ThemeError, parse_theme};

// ----------------------------------------------------------------- helpers

fn preset_source(id: &str) -> &'static str {
    BUILTIN_PRESETS.iter().find(|p| p.id == id).unwrap().source
}

fn replace_once(text: &str, from: &str, to: &str) -> String {
    assert_eq!(
        text.matches(from).count(),
        1,
        "pattern {from:?} must occur exactly once"
    );
    text.replacen(from, to, 1)
}

/// A valid user theme derived from windowsRICE Default with its own id/name.
fn user_theme(id: &str, name: &str) -> String {
    let t = replace_once(
        preset_source(DEFAULT_THEME_ID),
        "id = \"windowsrice-default\"",
        &format!("id = \"{id}\""),
    );
    replace_once(
        &t,
        "name = \"windowsRICE Default\"",
        &format!("name = \"{name}\""),
    )
}

fn write(dir: &Path, name: &str, content: impl AsRef<[u8]>) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, content).unwrap();
    p
}

/// Records every theme it is asked to apply and can be told to refuse ids.
#[derive(Default)]
struct RecordingSink {
    applied: Vec<String>,
    refuse: Vec<String>,
}

impl ThemeSink for RecordingSink {
    fn apply(&mut self, theme: &ResolvedTheme) -> Result<(), String> {
        self.applied.push(theme.id.clone());
        if self.refuse.contains(&theme.id) {
            Err(format!("renderer refused {}", theme.id))
        } else {
            Ok(())
        }
    }
}

fn selection(id: &str) -> ThemeSelection {
    ThemeSelection {
        active: id.into(),
        mode: ThemeModePreference::Auto,
    }
}

fn has_notice(notices: &[Notice], f: impl Fn(&Notice) -> bool) -> bool {
    notices.iter().any(f)
}

// ----------------------------------------------------------------- catalog

#[test]
fn default_ids_agree_between_core_and_theme() {
    assert_eq!(CORE_DEFAULT_ID, DEFAULT_THEME_ID);
}

#[test]
fn builtin_catalog_has_eight_valid_themes_with_versions() {
    let c = ThemeCatalog::builtin_only();
    assert_eq!(c.entries().len(), BUILTIN_COUNT);
    assert!(c.issues().is_empty());
    for e in c.entries() {
        assert!(e.is_builtin());
        assert_eq!(e.version.id, e.id());
        assert_eq!(e.version.version.to_string(), "1.0.0");
        assert_eq!(e.version.schema_version, 1);
        assert!(e.version.fingerprint.starts_with("fnv1a64:"));
    }
    // Fingerprints are deterministic and distinct per preset.
    let again = ThemeCatalog::builtin_only();
    let fps: std::collections::HashSet<_> = c
        .entries()
        .iter()
        .map(|e| e.version.fingerprint.clone())
        .collect();
    assert_eq!(fps.len(), BUILTIN_COUNT);
    for (a, b) in c.entries().iter().zip(again.entries()) {
        assert_eq!(a.version, b.version);
    }
}

#[test]
fn missing_user_dir_is_not_an_issue() {
    let tmp = tempfile::tempdir().unwrap();
    let c = ThemeCatalog::load(Some(&tmp.path().join("does-not-exist")));
    assert_eq!(c.entries().len(), BUILTIN_COUNT);
    assert!(c.issues().is_empty());
}

#[test]
fn loads_valid_user_theme_and_identifies_its_version() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write(tmp.path(), "ocean.toml", user_theme("my-ocean", "My Ocean"));
    write(tmp.path(), "notes.txt", "not a theme");
    let c = ThemeCatalog::load(Some(tmp.path()));
    assert!(c.issues().is_empty(), "{:?}", c.issues());
    let e = c.get("my-ocean").expect("user theme loaded");
    assert_eq!(e.source, ThemeSource::User { path });
    assert_eq!(e.version.version.to_string(), "1.0.0");
    // Same tokens as the default, but a different file → different fingerprint.
    assert_ne!(
        e.version.fingerprint,
        c.get(DEFAULT_THEME_ID).unwrap().version.fingerprint
    );
}

#[test]
fn rejects_bad_user_theme_files_with_reasons() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    write(
        d,
        "a-bold.toml",
        replace_once(
            &user_theme("bold-one", "Bold One"),
            "weight_strong = 600",
            "weight_strong = 800",
        ),
    );
    write(d, "b-corrupt.toml", "[meta\nid = ");
    write(d, "c-binary.toml", [0xFF_u8, 0xFE, 0x00, 0x41]);
    write(
        d,
        "d-builtin-id.toml",
        user_theme("windowsrice-glass", "Not Glass"),
    );
    write(
        d,
        "e-builtin-name.toml",
        user_theme("fake-default", "windowsRICE Default"),
    );
    write(d, "f-dup-1.toml", user_theme("dup", "Dup One"));
    write(d, "g-dup-2.toml", user_theme("dup", "Dup Two"));
    write(d, "h-huge.toml", vec![b'#'; MAX_THEME_FILE_BYTES + 1]);
    std::fs::create_dir_all(d.join("i-folder.toml")).unwrap();
    let future = replace_once(
        &user_theme("future", "Future"),
        "schema_version = 1",
        "schema_version = 2",
    );
    write(
        d,
        "j-future.toml",
        format!("{future}\n[effects]\nparallax = true\n"),
    );

    let c = ThemeCatalog::load(Some(d));
    let kind_of = |file: &str| {
        &c.issues()
            .iter()
            .find(|i| i.path.file_name().unwrap() == file)
            .unwrap_or_else(|| panic!("no issue for {file}: {:?}", c.issues()))
            .kind
    };
    assert!(matches!(
        kind_of("a-bold.toml"),
        IssueKind::Theme(ThemeError::Invalid(_))
    ));
    assert!(matches!(
        kind_of("b-corrupt.toml"),
        IssueKind::Theme(ThemeError::Parse(_))
    ));
    assert!(matches!(kind_of("c-binary.toml"), IssueKind::NotUtf8));
    assert!(matches!(
        kind_of("d-builtin-id.toml"),
        IssueKind::ReservedBuiltinId
    ));
    assert!(matches!(
        kind_of("e-builtin-name.toml"),
        IssueKind::ReservedBuiltinName { .. }
    ));
    assert!(matches!(
        kind_of("g-dup-2.toml"),
        IssueKind::DuplicateId { .. }
    ));
    assert!(matches!(kind_of("h-huge.toml"), IssueKind::TooLarge { .. }));
    assert!(matches!(
        kind_of("i-folder.toml"),
        IssueKind::NotRegularFile
    ));
    assert!(matches!(
        kind_of("j-future.toml"),
        IssueKind::Theme(ThemeError::Incompatible {
            found: 2,
            supported: 1,
            ..
        })
    ));

    // The first duplicate is kept; built-ins are not replaced.
    assert_eq!(c.get("dup").unwrap().theme.meta.name, "Dup One");
    assert!(c.get("windowsrice-glass").unwrap().is_builtin());
    assert_eq!(c.entries().len(), BUILTIN_COUNT + 1);
    // Issues name the declared id when it can be read, and render readable text.
    assert_eq!(
        c.issue_for("bold-one")
            .map(|i| i.path.file_name().unwrap().to_owned()),
        Some("a-bold.toml".into())
    );
    assert!(
        c.issue_for("bold-one")
            .unwrap()
            .to_string()
            .contains("weight 800 exceeds the maximum of 600")
    );
}

#[test]
fn newer_schema_that_still_parses_keeps_phase2_validation_error() {
    let t = replace_once(
        &user_theme("x", "X"),
        "schema_version = 1",
        "schema_version = 2",
    );
    assert!(matches!(parse_theme(&t), Err(ThemeError::Invalid(_))));
}

// ------------------------------------------------------------------ modes

#[test]
fn mode_resolution() {
    let default = parse_theme(preset_source("windowsrice-default"))
        .unwrap()
        .theme;
    let light_only = parse_theme(preset_source("windowsrice-light"))
        .unwrap()
        .theme;

    assert_eq!(
        resolve_mode(&default, ThemeModePreference::Auto, None).0,
        Mode::Dark
    );
    assert_eq!(
        resolve_mode(&default, ThemeModePreference::Auto, Some(Mode::Light)).0,
        Mode::Light
    );
    assert_eq!(
        resolve_mode(&default, ThemeModePreference::Light, Some(Mode::Dark)).0,
        Mode::Light
    );
    assert_eq!(
        resolve_mode(&light_only, ThemeModePreference::Auto, Some(Mode::Dark)).0,
        Mode::Light
    );
    let (m, notice) = resolve_mode(&light_only, ThemeModePreference::Dark, None);
    assert_eq!(m, Mode::Light);
    assert!(matches!(
        notice,
        Some(Notice::ModeUnavailable {
            requested: Mode::Dark,
            used: Mode::Light,
            ..
        })
    ));
}

// ----------------------------------------------------------------- engine

#[test]
fn start_activates_the_selected_theme() {
    let mut sink = RecordingSink::default();
    let (engine, notices) = ThemeEngine::start(
        ThemeCatalog::builtin_only(),
        &selection("windowsrice-hypr"),
        None,
        &mut sink,
    );
    assert_eq!(engine.applied().unwrap().id, "windowsrice-hypr");
    assert_eq!(engine.applied().unwrap().mode, Mode::Dark);
    assert_eq!(sink.applied, ["windowsrice-hypr"]);
    assert!(notices.is_empty(), "{notices:?}");
}

#[test]
fn start_falls_back_to_default_for_missing_theme() {
    let mut sink = RecordingSink::default();
    let (engine, notices) = ThemeEngine::start(
        ThemeCatalog::builtin_only(),
        &selection("gone-theme"),
        None,
        &mut sink,
    );
    assert_eq!(engine.applied().unwrap().id, DEFAULT_THEME_ID);
    assert!(has_notice(
        &notices,
        |n| matches!(n, Notice::ThemeNotFound { requested } if requested == "gone-theme")
    ));
    assert!(has_notice(&notices, |n| matches!(
        n,
        Notice::FellBackToDefault { .. }
    )));
    assert!(notices.iter().any(|n| n.level() == NoticeLevel::Error));
}

#[test]
fn start_falls_back_to_default_for_rejected_user_theme_and_explains_why() {
    let tmp = tempfile::tempdir().unwrap();
    write(
        tmp.path(),
        "bad.toml",
        replace_once(
            &user_theme("bad-one", "Bad One"),
            "line_height = 1.45",
            "line_height = 1.0",
        ),
    );
    let mut sink = NoopSink;
    let (engine, notices) = ThemeEngine::start(
        ThemeCatalog::load(Some(tmp.path())),
        &selection("bad-one"),
        None,
        &mut sink,
    );
    assert_eq!(engine.applied().unwrap().id, DEFAULT_THEME_ID);
    let reason = notices
        .iter()
        .find_map(|n| match n {
            Notice::ThemeUnavailable { requested, reason } if requested == "bad-one" => {
                Some(reason.clone())
            }
            _ => None,
        })
        .expect("unavailable notice");
    assert!(reason.contains("line_height"), "{reason}");
}

#[test]
fn start_falls_back_when_renderer_refuses_selected_theme() {
    let mut sink = RecordingSink {
        refuse: vec!["windowsrice-glass".into()],
        ..Default::default()
    };
    let (engine, notices) = ThemeEngine::start(
        ThemeCatalog::builtin_only(),
        &selection("windowsrice-glass"),
        None,
        &mut sink,
    );
    assert_eq!(engine.applied().unwrap().id, DEFAULT_THEME_ID);
    assert_eq!(sink.applied, ["windowsrice-glass", DEFAULT_THEME_ID]);
    assert!(has_notice(&notices, |n| matches!(
        n,
        Notice::ThemeApplyFailed { .. }
    )));
}

#[test]
fn start_reports_when_even_default_is_refused() {
    let mut sink = RecordingSink {
        refuse: vec![DEFAULT_THEME_ID.into()],
        ..Default::default()
    };
    let (engine, notices) = ThemeEngine::start(
        ThemeCatalog::builtin_only(),
        &selection(DEFAULT_THEME_ID),
        None,
        &mut sink,
    );
    // A refused theme is never reported as applied.
    assert!(engine.applied().is_none());
    assert!(matches!(engine.status(), ActivationStatus::Failed { .. }));
    assert_eq!(engine.selected().active, DEFAULT_THEME_ID);
    assert!(has_notice(&notices, |n| matches!(
        n,
        Notice::ActivationFailed { kept: None, .. }
    )));
}

#[test]
fn prepared_engine_is_pending_and_applies_nothing() {
    let engine = ThemeEngine::prepare(ThemeCatalog::builtin_only(), &selection("windowsrice-hypr"));
    assert_eq!(engine.status(), &ActivationStatus::Pending);
    assert!(engine.applied().is_none());
    assert_eq!(engine.selected().active, "windowsrice-hypr");
}

#[test]
fn all_candidates_refused_keeps_selection_and_reports_failure() {
    let mut sink = RecordingSink {
        refuse: vec!["windowsrice-glass".into(), DEFAULT_THEME_ID.into()],
        ..Default::default()
    };
    let (engine, notices) = ThemeEngine::start(
        ThemeCatalog::builtin_only(),
        &selection("windowsrice-glass"),
        None,
        &mut sink,
    );
    assert!(engine.applied().is_none());
    assert!(!engine.is_fallback());
    assert_eq!(engine.selected().active, "windowsrice-glass");
    assert_eq!(sink.applied, ["windowsrice-glass", DEFAULT_THEME_ID]);
    let attempted = notices
        .iter()
        .find_map(|n| match n {
            Notice::ActivationFailed {
                attempted,
                kept: None,
                ..
            } => Some(attempted.clone()),
            _ => None,
        })
        .expect("explicit activation failure");
    assert_eq!(attempted, ["windowsrice-glass", DEFAULT_THEME_ID]);
    assert!(!has_notice(&notices, |n| matches!(
        n,
        Notice::FellBackToDefault { .. }
    )));
}

#[test]
fn reapplying_selection_keeps_last_known_good_theme_when_all_refused() {
    let mut sink = RecordingSink::default();
    let (mut engine, _) = ThemeEngine::start(
        ThemeCatalog::builtin_only(),
        &selection("windowsrice-ambient"),
        None,
        &mut sink,
    );
    engine.set_selection(selection("windowsrice-hypr"));
    sink.refuse = vec!["windowsrice-hypr".into(), DEFAULT_THEME_ID.into()];
    let notices = engine.apply_selection(None, &mut sink);

    // The known-good theme was restored and is still the applied one.
    assert_eq!(engine.applied().unwrap().id, "windowsrice-ambient");
    assert!(matches!(engine.status(), ActivationStatus::Failed { .. }));
    assert_eq!(engine.selected().active, "windowsrice-hypr");
    assert!(engine.is_fallback());
    assert_eq!(sink.applied.last().unwrap(), "windowsrice-ambient");
    assert!(has_notice(&notices, |n| matches!(
        n,
        Notice::ActivationFailed { kept: Some(k), .. } if k == "windowsrice-ambient"
    )));
}

#[test]
fn failed_restore_leaves_no_theme_reported_as_applied() {
    let mut sink = RecordingSink::default();
    let (mut engine, _) = ThemeEngine::start(
        ThemeCatalog::builtin_only(),
        &selection("windowsrice-ambient"),
        None,
        &mut sink,
    );
    sink.refuse = vec!["windowsrice-glass".into(), "windowsrice-ambient".into()];
    let err = engine
        .activate(
            "windowsrice-glass",
            ThemeModePreference::Auto,
            None,
            &mut sink,
        )
        .unwrap_err();
    assert!(matches!(
        &err,
        ActivationError::ApplyFailed {
            restored: None,
            restore_error: Some(_),
            ..
        }
    ));
    assert!(engine.applied().is_none());
    assert!(matches!(engine.status(), ActivationStatus::Failed { .. }));
    assert!(err.to_string().contains("no theme is confirmed on screen"));

    // Once the renderer recovers, re-applying the selection succeeds.
    sink.refuse.clear();
    engine.apply_selection(None, &mut sink);
    assert_eq!(engine.status(), &ActivationStatus::Applied);
    assert_eq!(engine.applied().unwrap().id, "windowsrice-ambient");
}

#[test]
fn fallback_is_distinguished_from_selection() {
    let (engine, _) = ThemeEngine::start(
        ThemeCatalog::builtin_only(),
        &selection("not-installed"),
        None,
        &mut NoopSink,
    );
    assert_eq!(engine.status(), &ActivationStatus::Applied);
    assert_eq!(engine.applied().unwrap().id, DEFAULT_THEME_ID);
    assert_eq!(engine.selected().active, "not-installed");
    assert!(engine.is_fallback());
}

#[test]
fn switching_themes_and_reverting() {
    let mut sink = RecordingSink::default();
    let (mut engine, _) = ThemeEngine::start(
        ThemeCatalog::builtin_only(),
        &selection(DEFAULT_THEME_ID),
        None,
        &mut sink,
    );

    let a = engine
        .activate(
            "windowsrice-minimal",
            ThemeModePreference::Light,
            None,
            &mut sink,
        )
        .unwrap();
    assert_eq!(
        (a.theme.id.as_str(), a.theme.mode),
        ("windowsrice-minimal", Mode::Light)
    );
    assert_eq!(engine.previous().unwrap().id, DEFAULT_THEME_ID);

    engine
        .activate(
            "windowsrice-amoled",
            ThemeModePreference::Auto,
            None,
            &mut sink,
        )
        .unwrap();
    assert_eq!(engine.applied().unwrap().id, "windowsrice-amoled");

    engine.revert(&mut sink).unwrap();
    assert_eq!(engine.applied().unwrap().id, "windowsrice-minimal");
    assert_eq!(sink.applied.last().unwrap(), "windowsrice-minimal");
}

#[test]
fn activating_missing_theme_leaves_state_unchanged() {
    let mut sink = RecordingSink::default();
    let (mut engine, _) = ThemeEngine::start(
        ThemeCatalog::builtin_only(),
        &selection("windowsrice-hypr"),
        None,
        &mut sink,
    );
    let err = engine
        .activate("nope", ThemeModePreference::Auto, None, &mut sink)
        .unwrap_err();
    assert!(matches!(err, ActivationError::NotFound { .. }));
    assert_eq!(engine.applied().unwrap().id, "windowsrice-hypr");
    assert_eq!(
        sink.applied,
        ["windowsrice-hypr"],
        "nothing was sent to the renderer"
    );
}

#[test]
fn failed_activation_restores_previous_theme() {
    let mut sink = RecordingSink::default();
    let (mut engine, _) = ThemeEngine::start(
        ThemeCatalog::builtin_only(),
        &selection("windowsrice-ambient"),
        None,
        &mut sink,
    );
    sink.refuse.push("windowsrice-glass".into());

    let err = engine
        .activate(
            "windowsrice-glass",
            ThemeModePreference::Auto,
            None,
            &mut sink,
        )
        .unwrap_err();
    match &err {
        ActivationError::ApplyFailed {
            restored,
            restore_error,
            ..
        } => {
            assert_eq!(restored.as_deref(), Some("windowsrice-ambient"));
            assert!(restore_error.is_none());
        }
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(engine.applied().unwrap().id, "windowsrice-ambient");
    assert_eq!(
        sink.applied,
        [
            "windowsrice-ambient",
            "windowsrice-glass",
            "windowsrice-ambient"
        ]
    );
    assert!(err.to_string().contains("remains applied"));
}

#[test]
fn glass_activation_surfaces_its_accessibility_warnings() {
    let mut sink = NoopSink;
    let (mut engine, _) = ThemeEngine::start(
        ThemeCatalog::builtin_only(),
        &selection(DEFAULT_THEME_ID),
        None,
        &mut sink,
    );
    let a = engine
        .activate(
            "windowsrice-glass",
            ThemeModePreference::Auto,
            None,
            &mut sink,
        )
        .unwrap();
    let warnings = a
        .notices
        .iter()
        .find_map(|n| match n {
            Notice::ThemeWarnings { warnings, .. } => Some(warnings.clone()),
            _ => None,
        })
        .expect("Glass must report warnings");
    assert!(warnings.iter().any(|w| w.contains("contrast drops to")));
    // Default has none.
    let a = engine
        .activate(DEFAULT_THEME_ID, ThemeModePreference::Auto, None, &mut sink)
        .unwrap();
    assert!(a.notices.is_empty());
}

// ---------------------------------------------------------------- service

struct Env {
    _tmp: tempfile::TempDir,
    config: PathBuf,
    themes: PathBuf,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    Env {
        config: tmp.path().join("cfg").join("config.toml"),
        themes: tmp.path().join("cfg").join("themes"),
        _tmp: tmp,
    }
}

fn start(e: &Env) -> ThemeService {
    ThemeService::start_with(&e.config, Some(&e.themes), None, &mut NoopSink)
}

fn backups(e: &Env) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(e.config.parent().unwrap())
        .unwrap()
        .map(|x| x.unwrap().path())
        .filter(|p| p.file_name().unwrap().to_string_lossy().ends_with(".bak"))
        .collect();
    v.sort();
    v
}

#[test]
fn service_startup_with_renderer_refusing_everything() {
    let e = env();
    let mut config = Config::default();
    config.theme.active = "windowsrice-hypr".into();
    config.save_atomic(&e.config).unwrap();
    let saved = std::fs::read_to_string(&e.config).unwrap();

    let mut sink = RecordingSink {
        refuse: vec!["windowsrice-hypr".into(), DEFAULT_THEME_ID.into()],
        ..Default::default()
    };
    let s = ThemeService::start_with(&e.config, Some(&e.themes), None, &mut sink);
    assert!(s.applied().is_none());
    assert!(matches!(s.status(), ActivationStatus::Failed { .. }));
    assert_eq!(s.selected().active, "windowsrice-hypr");
    assert!(has_notice(s.notices(), |n| matches!(
        n,
        Notice::ActivationFailed { .. }
    )));
    assert_eq!(
        std::fs::read_to_string(&e.config).unwrap(),
        saved,
        "nothing written"
    );
}

#[test]
fn successful_select_updates_selection_and_applied_together() {
    let e = env();
    let mut s = start(&e);
    let (theme, _) = s
        .select(
            "windowsrice-minimal",
            ThemeModePreference::Auto,
            None,
            &mut NoopSink,
        )
        .unwrap();
    assert_eq!(theme.id, "windowsrice-minimal");
    assert_eq!(s.selected().active, "windowsrice-minimal");
    assert_eq!(s.applied().unwrap().id, "windowsrice-minimal");
    assert_eq!(s.status(), &ActivationStatus::Applied);
}

#[test]
fn first_run_uses_default_and_writes_nothing() {
    let e = env();
    let s = start(&e);
    assert_eq!(s.applied().unwrap().id, DEFAULT_THEME_ID);
    assert!(has_notice(s.notices(), |n| matches!(
        n,
        Notice::ConfigMissing
    )));
    assert!(!e.config.exists());
    assert!(!e.config.parent().unwrap().exists());
}

#[test]
fn selection_is_saved_and_restored_on_next_start() {
    let e = env();
    let mut s = start(&e);
    s.select(
        "windowsrice-macos-inspired",
        ThemeModePreference::Light,
        None,
        &mut NoopSink,
    )
    .unwrap();
    assert_eq!(s.applied().unwrap().mode, Mode::Light);

    let on_disk = Config::load_or_default(&e.config).0;
    assert_eq!(on_disk.theme.active, "windowsrice-macos-inspired");
    assert_eq!(on_disk.theme.mode, ThemeModePreference::Light);

    let again = start(&e);
    assert_eq!(again.applied().unwrap().id, "windowsrice-macos-inspired");
    assert_eq!(again.applied().unwrap().mode, Mode::Light);
    assert!(again.notices().is_empty(), "{:?}", again.notices());
}

#[test]
fn user_theme_can_be_selected_and_survives_restart() {
    let e = env();
    write(&e.themes, "ocean.toml", user_theme("my-ocean", "My Ocean"));
    let mut s = start(&e);
    s.select("my-ocean", ThemeModePreference::Auto, None, &mut NoopSink)
        .unwrap();
    assert_eq!(start(&e).applied().unwrap().id, "my-ocean");
}

#[test]
fn deleted_user_theme_falls_back_without_discarding_the_selection() {
    let e = env();
    let file = write(&e.themes, "ocean.toml", user_theme("my-ocean", "My Ocean"));
    start(&e)
        .select("my-ocean", ThemeModePreference::Auto, None, &mut NoopSink)
        .unwrap();
    let saved = std::fs::read_to_string(&e.config).unwrap();

    std::fs::remove_file(file).unwrap();
    let s = start(&e);
    assert_eq!(s.applied().unwrap().id, DEFAULT_THEME_ID);
    assert!(has_notice(
        s.notices(),
        |n| matches!(n, Notice::FellBackToDefault { requested } if requested == "my-ocean")
    ));
    // The configuration still names the user's theme and the file is untouched.
    assert_eq!(s.config().theme.active, "my-ocean");
    assert_eq!(std::fs::read_to_string(&e.config).unwrap(), saved);
}

#[test]
fn corrupted_user_theme_falls_back_and_selection_survives_repair() {
    let e = env();
    let file = write(&e.themes, "ocean.toml", user_theme("my-ocean", "My Ocean"));
    start(&e)
        .select("my-ocean", ThemeModePreference::Auto, None, &mut NoopSink)
        .unwrap();

    std::fs::write(
        &file,
        replace_once(
            &user_theme("my-ocean", "My Ocean"),
            "accent = \"#5BA4E6\"",
            "accent = \"#5BA4E\"",
        ),
    )
    .unwrap();
    let s = start(&e);
    assert_eq!(s.applied().unwrap().id, DEFAULT_THEME_ID);
    assert!(has_notice(
        s.notices(),
        |n| matches!(n, Notice::ThemeUnavailable { requested, .. } if requested == "my-ocean")
    ));

    std::fs::write(&file, user_theme("my-ocean", "My Ocean")).unwrap();
    assert_eq!(start(&e).applied().unwrap().id, "my-ocean");
}

#[test]
fn failed_selection_keeps_previous_theme_and_config() {
    let e = env();
    let mut s = start(&e);
    s.select(
        "windowsrice-hypr",
        ThemeModePreference::Auto,
        None,
        &mut NoopSink,
    )
    .unwrap();
    let saved = std::fs::read_to_string(&e.config).unwrap();

    let err = s
        .select(
            "missing-theme",
            ThemeModePreference::Auto,
            None,
            &mut NoopSink,
        )
        .unwrap_err();
    assert!(matches!(
        err,
        SelectError::Activation(ActivationError::NotFound { .. })
    ));

    let mut refusing = RecordingSink {
        refuse: vec!["windowsrice-glass".into()],
        ..Default::default()
    };
    let err = s
        .select(
            "windowsrice-glass",
            ThemeModePreference::Auto,
            None,
            &mut refusing,
        )
        .unwrap_err();
    assert!(matches!(
        err,
        SelectError::Activation(ActivationError::ApplyFailed { .. })
    ));

    assert_eq!(s.applied().unwrap().id, "windowsrice-hypr");
    assert_eq!(s.config().theme.active, "windowsrice-hypr");
    assert_eq!(std::fs::read_to_string(&e.config).unwrap(), saved);
}

#[test]
fn save_failure_reverts_the_applied_theme() {
    let e = env();
    let mut s = start(&e);
    s.select(
        "windowsrice-hypr",
        ThemeModePreference::Auto,
        None,
        &mut NoopSink,
    )
    .unwrap();
    // Make the save fail: the temporary file path is occupied by a folder.
    std::fs::create_dir_all(e.config.with_file_name("config.toml.tmp")).unwrap();

    let mut sink = RecordingSink::default();
    let err = s
        .select(
            "windowsrice-light",
            ThemeModePreference::Auto,
            None,
            &mut sink,
        )
        .unwrap_err();
    assert!(
        matches!(
            err,
            SelectError::PersistFailed {
                revert_error: None,
                ..
            }
        ),
        "{err}"
    );
    assert_eq!(sink.applied, ["windowsrice-light", "windowsrice-hypr"]);
    assert_eq!(s.applied().unwrap().id, "windowsrice-hypr");
    assert_eq!(s.config().theme.active, "windowsrice-hypr");
    assert_eq!(
        Config::load_or_default(&e.config).0.theme.active,
        "windowsrice-hypr"
    );
}

#[test]
fn v1_config_is_migrated_and_original_preserved_before_first_save() {
    let e = env();
    let v1 = "schema_version = 1\n[general]\nlanguage = \"korean\"\nactive_theme = \"windowsRICE AMOLED\"\n[launcher]\nhotkey = \"Alt+Shift+K\"\n";
    write(e.config.parent().unwrap(), "config.toml", v1);

    let mut s = start(&e);
    assert_eq!(s.applied().unwrap().id, "windowsrice-amoled");
    assert!(has_notice(s.notices(), |n| matches!(
        n,
        Notice::ConfigMigrated {
            from_version: 1,
            ..
        }
    )));
    assert_eq!(
        std::fs::read_to_string(&e.config).unwrap(),
        v1,
        "startup writes nothing"
    );

    let (_, notices) = s
        .select(
            "windowsrice-ambient",
            ThemeModePreference::Auto,
            None,
            &mut NoopSink,
        )
        .unwrap();
    assert!(has_notice(&notices, |n| matches!(
        n,
        Notice::OriginalConfigPreserved { .. }
    )));
    let preserved: Vec<_> = backups(&e)
        .into_iter()
        .filter(|p| p.to_string_lossy().contains(".v1-"))
        .collect();
    assert_eq!(preserved.len(), 1);
    assert_eq!(std::fs::read_to_string(&preserved[0]).unwrap(), v1);

    // Migrated values survive the save.
    let now = Config::load_or_default(&e.config).0;
    assert_eq!(
        now.schema_version,
        windowsrice_core::config::CONFIG_SCHEMA_VERSION
    );
    assert_eq!(now.theme.active, "windowsrice-ambient");
    assert_eq!(now.launcher.hotkey.to_string(), "Alt+Shift+K");

    // The preserved copy is made once, not on every save.
    s.select(
        "windowsrice-hypr",
        ThemeModePreference::Auto,
        None,
        &mut NoopSink,
    )
    .unwrap();
    assert_eq!(
        backups(&e)
            .iter()
            .filter(|p| p.to_string_lossy().contains(".v1-"))
            .count(),
        1
    );
}

#[test]
fn corrupt_config_is_never_lost() {
    let e = env();
    let garbage = "theme = [[[ not toml";
    write(e.config.parent().unwrap(), "config.toml", garbage);

    let mut s = start(&e);
    assert_eq!(s.applied().unwrap().id, DEFAULT_THEME_ID);
    assert!(has_notice(s.notices(), |n| matches!(
        n,
        Notice::ConfigInvalid { .. }
    )));
    assert_eq!(std::fs::read_to_string(&e.config).unwrap(), garbage);

    s.select(
        "windowsrice-minimal",
        ThemeModePreference::Auto,
        None,
        &mut NoopSink,
    )
    .unwrap();
    s.select(
        "windowsrice-hypr",
        ThemeModePreference::Auto,
        None,
        &mut NoopSink,
    )
    .unwrap();
    let preserved: Vec<_> = backups(&e)
        .into_iter()
        .filter(|p| p.to_string_lossy().contains(".invalid-"))
        .collect();
    assert_eq!(preserved.len(), 1);
    assert_eq!(
        std::fs::read_to_string(&preserved[0]).unwrap(),
        garbage,
        "original bytes survive later saves"
    );
}

#[test]
fn future_config_schema_is_not_overwritten_silently() {
    let e = env();
    let future = "schema_version = 9\n[theme]\nactive = \"windowsrice-hypr\"\n";
    write(e.config.parent().unwrap(), "config.toml", future);
    let mut s = start(&e);
    assert_eq!(s.applied().unwrap().id, DEFAULT_THEME_ID);
    assert!(has_notice(
        s.notices(),
        |n| matches!(n, Notice::ConfigInvalid { error } if error.contains("newer"))
    ));
    s.select(
        "windowsrice-light",
        ThemeModePreference::Auto,
        None,
        &mut NoopSink,
    )
    .unwrap();
    let preserved: Vec<_> = backups(&e)
        .into_iter()
        .filter(|p| p.to_string_lossy().contains(".invalid-"))
        .collect();
    assert_eq!(std::fs::read_to_string(&preserved[0]).unwrap(), future);
}

#[test]
fn notices_render_readable_text() {
    let e = env();
    write(&e.themes, "x.toml", "[meta\n");
    let s = start(&e);
    let text: Vec<String> = s.notices().iter().map(ToString::to_string).collect();
    assert!(
        text.iter()
            .any(|t| t.starts_with("No configuration file was found"))
    );
    assert!(
        text.iter()
            .any(|t| t.contains("x.toml") && t.contains("rejected"))
    );
}

#[test]
fn reapply_follows_system_mode_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let mut service = ThemeService::start_with(&config_path, None, Some(Mode::Dark), &mut NoopSink);
    assert_eq!(service.applied().unwrap().mode, Mode::Dark);
    let notices = service.reapply(Some(Mode::Light), &mut NoopSink);
    assert!(
        notices
            .iter()
            .all(|n| !matches!(n, Notice::ActivationFailed { .. }))
    );
    let applied = service.applied().unwrap();
    assert_eq!(applied.id, DEFAULT_THEME_ID);
    assert_eq!(applied.mode, Mode::Light);
    assert!(
        !config_path.exists(),
        "reapply must not write the configuration"
    );
}

#[test]
fn update_config_saves_valid_bar_changes_only() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let mut service = ThemeService::start_with(&config_path, None, Some(Mode::Dark), &mut NoopSink);
    // Invalid change: rejected, nothing written, memory unchanged.
    let err = service
        .update_config(|c| c.bar.height = Some(500))
        .unwrap_err();
    assert!(err.contains("bar.height"), "{err}");
    assert!(!config_path.exists());
    assert_eq!(service.config().bar.height, None);
    // Valid change: saved and reloaded identically; theme selection untouched.
    service
        .update_config(|c| {
            c.bar.height = Some(40);
            c.bar.auto_hide = true;
            c.theme.active = "windowsrice-hypr".into(); // ignored: use select()
        })
        .unwrap();
    let saved = Config::load_or_default(&config_path).0;
    assert_eq!(saved.bar.height, Some(40));
    assert!(saved.bar.auto_hide);
    assert_eq!(saved.theme.active, DEFAULT_THEME_ID);
    assert_eq!(&saved, service.config());
}

#[test]
fn update_config_preserves_a_v2_original_before_first_save() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let v2 = "schema_version = 2\n[theme]\nactive = \"windowsrice-glass\"\n";
    std::fs::write(&config_path, v2).unwrap();
    let mut service = ThemeService::start_with(&config_path, None, Some(Mode::Dark), &mut NoopSink);
    let notices = service.update_config(|c| c.bar.spacing = 6).unwrap();
    assert!(
        notices
            .iter()
            .any(|n| matches!(n, Notice::OriginalConfigPreserved { .. }))
    );
    let preserved: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().contains(".v2-"))
        .collect();
    assert_eq!(preserved.len(), 1);
    assert_eq!(std::fs::read_to_string(&preserved[0]).unwrap(), v2);
    let saved = Config::load_or_default(&config_path).0;
    assert_eq!(saved.theme.active, "windowsrice-glass");
    assert_eq!(saved.bar.spacing, 6);
}
