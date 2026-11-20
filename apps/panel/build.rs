//! Embeds the application icon and Windows version information.
//!
//! The resource script is generated with the package version and compiled
//! with the Windows SDK resource compiler (`rc.exe`): `$RC` when set,
//! otherwise the newest x64 `rc.exe` under the Windows Kits folder. Without
//! it the build continues with a warning and the executable has no icon or
//! version information. No crates are needed for this.

use std::path::{Path, PathBuf};
use std::process::Command;

fn find_rc() -> Option<PathBuf> {
    if let Some(rc) = std::env::var_os("RC") {
        let p = PathBuf::from(rc);
        if p.exists() {
            return Some(p);
        }
    }
    let kits = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut versions: Vec<PathBuf> = std::fs::read_dir(kits)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path().join("x64").join("rc.exe"))
        .filter(|p| p.exists())
        .collect();
    versions.sort();
    versions.pop()
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let icon = manifest.join("../../assets/windowsrice.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=RC");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let version = std::env::var("CARGO_PKG_VERSION").expect("set by cargo");
    let mut parts: Vec<u16> = version
        .split(['.', '-'])
        .take(3)
        .map(|p| p.parse().unwrap_or(0))
        .collect();
    parts.resize(3, 0);
    let (a, b, c) = (parts[0], parts[1], parts[2]);
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("set by cargo"));
    let rc_file = out.join("windowsrice.rc");
    let res_file = out.join("windowsrice.res");
    let icon_line = if icon.exists() {
        format!(
            "1 ICON \"{}\"\n",
            icon.canonicalize()
                .unwrap_or(icon.clone())
                .display()
                .to_string()
                .trim_start_matches(r"\\?\")
                .replace('\\', "\\\\")
        )
    } else {
        println!("cargo:warning=assets/windowsrice.ico not found; building without an icon");
        String::new()
    };
    let script = format!(
        r#"{icon_line}
1 VERSIONINFO
FILEVERSION {a},{b},{c},0
PRODUCTVERSION {a},{b},{c},0
FILEFLAGSMASK 0x3F
FILEFLAGS 0
FILEOS 0x40004
FILETYPE 1
FILESUBTYPE 0
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "windowsRICE"
      VALUE "FileDescription", "windowsRICE Bar"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "windowsrice-panel"
      VALUE "OriginalFilename", "windowsrice-panel.exe"
      VALUE "ProductName", "windowsRICE"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    );
    std::fs::write(&rc_file, script).expect("write resource script");
    let Some(rc) = find_rc() else {
        println!(
            "cargo:warning=rc.exe (Windows SDK) not found; building without icon and version information"
        );
        return;
    };
    let status = Command::new(&rc)
        .arg("/nologo")
        .arg("/fo")
        .arg(&res_file)
        .arg(&rc_file)
        .status();
    match status {
        Ok(s) if s.success() => {
            println!("cargo:rustc-link-arg-bins={}", res_file.display());
        }
        other => {
            println!(
                "cargo:warning=rc.exe failed ({other:?}); building without icon and version information"
            );
        }
    }
}
