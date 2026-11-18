//! Plain-text rendering of the diagnostics report.

use std::fmt::Write;

use windowsrice_windows::display::Resolution;
use windowsrice_windows::{DiagnosticsReport, Section};

const NA: &str = "N/A";

pub fn bytes(n: u64) -> String {
    #[allow(clippy::cast_precision_loss)] // display only
    let gib = n as f64 / 1024.0 / 1024.0 / 1024.0;
    format!("{} bytes ({gib:.2} GiB)", group_digits(n))
}

pub fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn res(r: Option<Resolution>) -> String {
    r.map_or_else(|| NA.into(), |r| format!("{} x {}", r.width, r.height))
}

fn hz(v: Option<f64>) -> String {
    v.map_or_else(|| NA.into(), |v| format!("{v:.3} Hz"))
}

fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| NA.into(), |v| v.to_string())
}

fn section<T>(out: &mut String, title: &str, s: &Section<T>, body: impl FnOnce(&mut String, &T)) {
    let _ = writeln!(out, "\n[{title}]");
    match s {
        Section::Available(v) => body(out, v),
        Section::Unavailable { error } => {
            let _ = writeln!(out, "  unavailable: {error}");
        }
    }
}

pub fn text(r: &DiagnosticsReport) -> String {
    let mut o = String::new();
    let _ = writeln!(
        o,
        "windowsRICE diagnostics {} (read-only)",
        r.windowsrice_version
    );
    let _ = writeln!(
        o,
        "Per-Monitor-v2 DPI aware: {}",
        r.dpi_awareness.per_monitor_v2
    );

    section(&mut o, "Operating system", &r.os, |o, os| {
        let _ = writeln!(
            o,
            "  Version            {}.{}.{}.{}",
            os.major,
            os.minor,
            os.build,
            opt(os.ubr)
        );
        let _ = writeln!(
            o,
            "  Feature update     {}",
            opt(os.display_version.as_deref())
        );
        let _ = writeln!(
            o,
            "  Edition            {:?} (product type 0x{:X})",
            os.edition, os.product_type
        );
        let _ = writeln!(o, "  Windows 11         {}", os.is_windows_11());
        let _ = writeln!(o, "  Architecture       {}", os.architecture);
        let _ = writeln!(
            o,
            "  Registry EditionID {}",
            opt(os.edition_id_registry.as_deref())
        );
        let _ = writeln!(
            o,
            "  Registry ProductName {} (reference only; known to be inaccurate on Windows 11)",
            opt(os.product_name_registry.as_deref())
        );
    });

    section(&mut o, "CPU", &r.cpu, |o, c| {
        let _ = writeln!(o, "  Name               {}", opt(c.name.as_deref()));
        let _ = writeln!(o, "  Physical cores     {}", opt(c.physical_cores));
        let _ = writeln!(o, "  Logical processors {}", opt(c.logical_processors));
        let _ = writeln!(o, "  Reported base clock {} MHz", opt(c.reported_base_mhz));
    });

    section(&mut o, "Memory", &r.memory, |o, m| {
        let _ = writeln!(
            o,
            "  Installed          {}",
            m.installed_bytes.map_or_else(|| NA.into(), bytes)
        );
        let _ = writeln!(o, "  Visible to Windows {}", bytes(m.visible_bytes));
        let _ = writeln!(
            o,
            "  Available (snapshot) {}",
            bytes(m.available_bytes_snapshot)
        );
        let _ = writeln!(o, "  Load (snapshot)    {}%", m.load_percent_snapshot);
    });

    section(&mut o, "Displays", &r.displays, |o, list| {
        for (i, d) in list.iter().enumerate() {
            let _ = writeln!(
                o,
                "  #{} {} {}{}",
                i + 1,
                d.gdi_device_name,
                opt(d.friendly_name.as_deref()),
                if d.is_primary { " (primary)" } else { "" }
            );
            let _ = writeln!(o, "     Native resolution     {}", res(d.native_resolution));
            let _ = writeln!(
                o,
                "     Preferred-mode refresh {} (EDID preferred timing, not the maximum)",
                hz(d.preferred_mode_refresh_hz)
            );
            let _ = writeln!(
                o,
                "     Current resolution    {}",
                res(d.current_resolution)
            );
            let _ = writeln!(o, "     Current refresh       {}", hz(d.current_refresh_hz));
            let _ = writeln!(o, "     Effective DPI         {}", opt(d.effective_dpi));
            let _ = writeln!(
                o,
                "     Scaling               {}",
                d.scale_percent
                    .map_or_else(|| NA.into(), |p| format!("{p}%"))
            );
            let _ = writeln!(
                o,
                "     Effective resolution  {}",
                res(d.effective_resolution)
            );
            let _ = writeln!(
                o,
                "     Rotation              {}",
                d.rotation_degrees
                    .map_or_else(|| NA.into(), |r| format!("{r}°"))
            );
            let _ = writeln!(
                o,
                "     Color depth           {}",
                d.bits_per_pixel
                    .map_or_else(|| NA.into(), |b| format!("{b} bpp"))
            );
            let rc = d.desktop_rect;
            let _ = writeln!(
                o,
                "     Desktop rect          ({}, {}) - ({}, {})",
                rc.left, rc.top, rc.right, rc.bottom
            );
        }
    });

    section(&mut o, "GPU adapters (DXGI)", &r.gpus, |o, list| {
        for g in list {
            let _ = writeln!(
                o,
                "  {} [{} {:04X}:{:04X} rev {:02X}]{}",
                g.description,
                g.vendor,
                g.vendor_id,
                g.device_id,
                g.revision,
                if g.is_software { " (software)" } else { "" }
            );
            let _ = writeln!(
                o,
                "     Dedicated video memory  {}",
                bytes(g.dedicated_video_memory_bytes)
            );
            let _ = writeln!(
                o,
                "     Shared system memory    {}",
                bytes(g.shared_system_memory_bytes)
            );
        }
    });

    section(
        &mut o,
        "AMD ADLX runtime (not loaded)",
        &r.amd_adlx,
        |o, a| {
            let _ = writeln!(o, "  Path     {}", a.library_path.display());
            let _ = writeln!(o, "  Present  {}", a.present);
            let _ = writeln!(o, "  File version     {}", opt(a.file_version.as_deref()));
            let _ = writeln!(
                o,
                "  Product version  {}",
                opt(a.product_version.as_deref())
            );
        },
    );

    let s = &r.session;
    let _ = writeln!(o, "\n[Session]");
    let _ = writeln!(o, "  Process elevated         {}", opt(s.process_elevated));
    let _ = writeln!(
        o,
        "  Winlogon Shell (HKLM)    {}",
        opt(s.winlogon_shell_machine.as_deref())
    );
    let _ = writeln!(
        o,
        "  Winlogon Shell (HKCU)    {}",
        s.winlogon_shell_user_override
            .as_deref()
            .unwrap_or("(not set)")
    );
    let _ = writeln!(
        o,
        "  Explorer taskbar present {}",
        s.explorer_taskbar_present
    );

    section(&mut o, "Capabilities", &r.capabilities, |o, caps| {
        for c in caps {
            let _ = writeln!(
                o,
                "  {:<52} {:<20} {:<20} {}",
                c.name,
                format!("{:?}", c.support),
                format!("{:?}", c.evidence),
                c.detail
            );
        }
    });
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_digits() {
        assert_eq!(group_digits(0), "0");
        assert_eq!(group_digits(999), "999");
        assert_eq!(group_digits(1000), "1,000");
        assert_eq!(group_digits(17_095_983_104), "17,095,983,104");
    }

    #[test]
    fn formats_bytes() {
        assert_eq!(bytes(17_179_869_184), "17,179,869,184 bytes (16.00 GiB)");
    }
}
