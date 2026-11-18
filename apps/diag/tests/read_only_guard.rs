//! Phase 1 guard: the diagnostics tool and the Windows integration crate must
//! remain read-only. This test fails if source code references APIs that
//! write to the registry or files, change system settings, change the shell
//! or startup configuration, start processes, hook input or request elevation.

use std::path::{Path, PathBuf};

const FORBIDDEN: &[&str] = &[
    // Registry writes
    "RegSetValue",
    "RegSetKeyValue",
    "RegCreateKey",
    "RegDeleteKey",
    "RegDeleteValue",
    "RegDeleteTree",
    "RegRestoreKey",
    "RegLoadKey",
    // System settings, shell, wallpaper, taskbar
    "SystemParametersInfo",
    "SHAppBarMessage",
    "IDesktopWallpaper",
    "SetDisplayConfig",
    "ChangeDisplaySettings",
    // Processes, elevation, startup
    "ShellExecute",
    "CreateProcess",
    "std::process::Command",
    "runas",
    "ITaskService",
    // Input
    "SetWindowsHookEx",
    "RegisterHotKey",
    "SendInput",
    // Window manipulation of other processes
    "SetWindowPos",
    "DwmSetWindowAttribute",
    // File writes
    "std::fs::write",
    "File::create",
    "OpenOptions",
    "create_dir",
    "save_atomic",
    "ensure_dirs",
    "init_file",
];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn read_only_sources_contain_no_write_apis() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let roots = [
        manifest.join("src"),
        manifest.join("../../crates/windowsrice-windows/src"),
    ];
    let mut files = Vec::new();
    for root in &roots {
        assert!(root.is_dir(), "missing {}", root.display());
        rust_files(root, &mut files);
    }
    assert!(
        files.len() >= 5,
        "expected source files, found {}",
        files.len()
    );

    let mut violations = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        for (n, line) in text.lines().enumerate() {
            for needle in FORBIDDEN {
                if line.contains(needle) {
                    violations.push(format!("{}:{}: {needle}", file.display(), n + 1));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "write APIs found:\n{}",
        violations.join("\n")
    );
}
