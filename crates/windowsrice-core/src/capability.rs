//! Platform-independent capability model.
//!
//! Given detected facts (Windows build and edition family), [`evaluate`]
//! classifies what windowsRICE may rely on. Each entry records where the
//! claim comes from so the UI never presents a hypothesis as a verified fact.

use serde::Serialize;

/// Windows edition family, derived from the `GetProductInfo` product type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "family", content = "product_type")]
pub enum EditionFamily {
    Home,
    Pro,
    Enterprise,
    Education,
    IotEnterprise,
    Unknown(u32),
}

impl EditionFamily {
    /// Maps a `GetProductInfo` product type (`PRODUCT_*` constant).
    ///
    /// Only `PRODUCT_PROFESSIONAL` (0x30) has been observed on the development
    /// machine; the other values come from the Windows SDK `winnt.h`.
    pub fn from_product_type(product_type: u32) -> Self {
        match product_type {
            // PRODUCT_CORE_N (0x62), _COUNTRYSPECIFIC, _SINGLELANGUAGE, PRODUCT_CORE (0x65)
            0x62..=0x65 => Self::Home,
            // PRODUCT_PROFESSIONAL, _N, PRO_WORKSTATION, _N, PRO_FOR_EDUCATION, _N
            0x30 | 0x31 | 0xA1 | 0xA2 | 0xA4 | 0xA5 => Self::Pro,
            // PRODUCT_ENTERPRISE, _N, _S (LTSC), _S_N
            0x04 | 0x1B | 0x7D | 0x7E => Self::Enterprise,
            // PRODUCT_EDUCATION, _N
            0x79 | 0x7A => Self::Education,
            // PRODUCT_IOTENTERPRISE, PRODUCT_IOTENTERPRISES
            0xBC | 0xBF => Self::IotEnterprise,
            other => Self::Unknown(other),
        }
    }

    fn is_enterprise_class(self) -> bool {
        matches!(
            self,
            Self::Enterprise | Self::Education | Self::IotEnterprise
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Support {
    Supported,
    Partial,
    ManagementOnly,
    OtherEditionOnly,
    CompanionOnly,
    Unsupported,
    ProtectedNative,
    ProhibitedByPolicy,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Evidence {
    /// Derived from Microsoft documentation for the detected build/edition.
    Documented,
    /// Documented, but behaviour still has to be tested on this machine.
    DocumentedUntested,
    /// A working assumption that has not been verified.
    Hypothesis,
    /// A windowsRICE project rule, not a technical limit.
    ProjectPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Capability {
    pub id: &'static str,
    pub name: &'static str,
    pub support: Support,
    pub evidence: Evidence,
    pub detail: String,
}

/// Facts needed to evaluate capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformFacts {
    pub build: u32,
    pub edition: EditionFamily,
}

const BUILD_WIN11_21H2: u32 = 22000;
const BUILD_WIN11_22H2: u32 = 22621;
const BUILD_WIN11_24H2: u32 = 26100;

fn cap(
    id: &'static str,
    name: &'static str,
    support: Support,
    evidence: Evidence,
    detail: impl Into<String>,
) -> Capability {
    Capability {
        id,
        name,
        support,
        evidence,
        detail: detail.into(),
    }
}

fn build_gated(
    id: &'static str,
    name: &'static str,
    facts: &PlatformFacts,
    min_build: u32,
    when_ok: Support,
    detail_ok: &str,
) -> Capability {
    if facts.build >= min_build {
        cap(id, name, when_ok, Evidence::Documented, detail_ok)
    } else {
        cap(
            id,
            name,
            Support::Unsupported,
            Evidence::Documented,
            format!("requires build {min_build}+, detected {}", facts.build),
        )
    }
}

/// Evaluates the capability list for the given platform facts.
pub fn evaluate(facts: &PlatformFacts) -> Vec<Capability> {
    use Evidence::*;
    use Support::*;

    let enterprise_class = facts.edition.is_enterprise_class();
    let edition_only = |id, name, feature: &str| {
        if enterprise_class {
            cap(
                id,
                name,
                Supported,
                Documented,
                format!(
                    "{feature} is available on this edition; windowsRICE does not use it in the initial version"
                ),
            )
        } else {
            cap(
                id,
                name,
                OtherEditionOnly,
                Documented,
                format!("{feature} requires Enterprise, Education or IoT Enterprise"),
            )
        }
    };

    vec![
        edition_only("shell_launcher", "Shell Launcher", "Shell Launcher"),
        edition_only("custom_logon", "Custom Logon", "Custom Logon"),
        if enterprise_class {
            cap(
                "personalization_csp",
                "Personalization CSP (lock screen / desktop image)",
                ManagementOnly,
                Documented,
                "available through MDM enrollment only",
            )
        } else {
            cap(
                "personalization_csp",
                "Personalization CSP (lock screen / desktop image)",
                Unsupported,
                Documented,
                "on Pro only with SharedPC SetEduPolicies or BootToCloud mode",
            )
        },
        cap(
            "lock_screen_user_image",
            "Per-user lock screen image",
            Partial,
            DocumentedUntested,
            "user-level setting; programmatic WinRT path from an unpackaged app is untested",
        ),
        cap(
            "sign_in_ui",
            "Sign-in UI and credential tiles",
            ProtectedNative,
            ProjectPolicy,
            "authentication UI stays native",
        ),
        cap(
            "winlogon_shell_replacement",
            "Winlogon shell replacement",
            ProhibitedByPolicy,
            ProjectPolicy,
            "Explorer remains the shell",
        ),
        cap(
            "appbar",
            "AppBar screen-space reservation",
            Supported,
            Documented,
            "SHAppBarMessage",
        ),
        cap(
            "register_hotkey",
            "Global hotkeys",
            Supported,
            Documented,
            "RegisterHotKey only; conflicts are reported at registration time",
        ),
        cap(
            "win_event_hook",
            "Window event hooks (out-of-context)",
            Supported,
            Documented,
            "SetWinEventHook with WINEVENT_OUTOFCONTEXT; no code is loaded into other processes",
        ),
        build_gated(
            "per_monitor_dpi_v2",
            "Per-Monitor v2 DPI awareness",
            facts,
            15063,
            Supported,
            "SetProcessDpiAwarenessContext",
        ),
        build_gated(
            "dwm_border_color",
            "DWM border color",
            facts,
            BUILD_WIN11_21H2,
            Partial,
            "documented for windowsRICE-owned windows; effect on other processes' windows is untested",
        ),
        build_gated(
            "dwm_corner_preference",
            "DWM corner preference",
            facts,
            BUILD_WIN11_21H2,
            Supported,
            "for windowsRICE-owned windows",
        ),
        build_gated(
            "dwm_system_backdrop",
            "DWM system backdrop (Mica/Acrylic)",
            facts,
            BUILD_WIN11_22H2,
            Supported,
            "for windowsRICE-owned windows",
        ),
        build_gated(
            "dwm_redirection_bitmap_alpha",
            "DWM redirection bitmap alpha",
            facts,
            BUILD_WIN11_24H2,
            Supported,
            "for windowsRICE-owned windows",
        ),
        cap(
            "virtual_desktop_manager",
            "Virtual desktops (documented API)",
            Partial,
            Documented,
            "IVirtualDesktopManager queries desktops and moves own windows; creating, switching and naming are undocumented",
        ),
        cap(
            "taskbar_skinning",
            "Taskbar / Start skinning",
            Unsupported,
            ProjectPolicy,
            "would require code injection into Explorer",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(caps: &'a [Capability], id: &str) -> &'a Capability {
        caps.iter().find(|c| c.id == id).unwrap()
    }

    const PRO_25H2: PlatformFacts = PlatformFacts {
        build: 26200,
        edition: EditionFamily::Pro,
    };

    #[test]
    fn maps_known_product_types() {
        assert_eq!(EditionFamily::from_product_type(0x30), EditionFamily::Pro);
        assert_eq!(
            EditionFamily::from_product_type(0x04),
            EditionFamily::Enterprise
        );
        assert_eq!(
            EditionFamily::from_product_type(0x79),
            EditionFamily::Education
        );
        assert_eq!(EditionFamily::from_product_type(0x65), EditionFamily::Home);
        assert_eq!(
            EditionFamily::from_product_type(0xBC),
            EditionFamily::IotEnterprise
        );
        assert_eq!(
            EditionFamily::from_product_type(0x9999),
            EditionFamily::Unknown(0x9999)
        );
    }

    #[test]
    fn pro_cannot_use_enterprise_only_features() {
        let caps = evaluate(&PRO_25H2);
        assert_eq!(
            find(&caps, "shell_launcher").support,
            Support::OtherEditionOnly
        );
        assert_eq!(
            find(&caps, "custom_logon").support,
            Support::OtherEditionOnly
        );
        assert_eq!(
            find(&caps, "personalization_csp").support,
            Support::Unsupported
        );
    }

    #[test]
    fn enterprise_reports_edition_features_as_available() {
        let caps = evaluate(&PlatformFacts {
            build: 26100,
            edition: EditionFamily::Enterprise,
        });
        assert_eq!(find(&caps, "shell_launcher").support, Support::Supported);
        assert_eq!(
            find(&caps, "personalization_csp").support,
            Support::ManagementOnly
        );
    }

    #[test]
    fn dwm_attributes_are_gated_by_build() {
        let new = evaluate(&PRO_25H2);
        assert_eq!(
            find(&new, "dwm_system_backdrop").support,
            Support::Supported
        );
        assert_eq!(
            find(&new, "dwm_redirection_bitmap_alpha").support,
            Support::Supported
        );

        let win10 = evaluate(&PlatformFacts {
            build: 19045,
            edition: EditionFamily::Pro,
        });
        assert_eq!(
            find(&win10, "dwm_border_color").support,
            Support::Unsupported
        );
        assert_eq!(
            find(&win10, "dwm_system_backdrop").support,
            Support::Unsupported
        );

        let w11_21h2 = evaluate(&PlatformFacts {
            build: 22000,
            edition: EditionFamily::Pro,
        });
        assert_eq!(
            find(&w11_21h2, "dwm_border_color").support,
            Support::Partial
        );
        assert_eq!(
            find(&w11_21h2, "dwm_system_backdrop").support,
            Support::Unsupported
        );
    }

    #[test]
    fn protected_surfaces_are_never_supported() {
        for facts in [
            PRO_25H2,
            PlatformFacts {
                build: 26200,
                edition: EditionFamily::Enterprise,
            },
        ] {
            let caps = evaluate(&facts);
            assert_eq!(find(&caps, "sign_in_ui").support, Support::ProtectedNative);
            assert_eq!(
                find(&caps, "winlogon_shell_replacement").support,
                Support::ProhibitedByPolicy
            );
        }
    }

    #[test]
    fn ids_are_unique() {
        let caps = evaluate(&PRO_25H2);
        let mut ids: Vec<_> = caps.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), caps.len());
    }
}
