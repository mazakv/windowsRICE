//! Theme version identification.
//!
//! A theme is identified by its id, its declared semantic version, its
//! schema version and a content fingerprint. The fingerprint changes whenever
//! the file's bytes change, even if the author forgot to bump `version`.

use std::fmt;
use std::str::FromStr;

use serde::Serialize;

/// Semantic version `MAJOR.MINOR.PATCH`, ordered numerically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ThemeVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid theme version \"{0}\": expected MAJOR.MINOR.PATCH")]
pub struct VersionParseError(pub String);

impl FromStr for ThemeVersion {
    type Err = VersionParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || VersionParseError(s.to_owned());
        let mut parts = s.split('.');
        let mut next = || -> Result<u32, VersionParseError> {
            let p = parts.next().ok_or_else(err)?;
            if p.is_empty()
                || !p.bytes().all(|b| b.is_ascii_digit())
                || (p.len() > 1 && p.starts_with('0'))
            {
                return Err(err());
            }
            p.parse().map_err(|_| err())
        };
        let v = ThemeVersion {
            major: next()?,
            minor: next()?,
            patch: next()?,
        };
        if parts.next().is_some() {
            return Err(err());
        }
        Ok(v)
    }
}

impl fmt::Display for ThemeVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Stable 64-bit FNV-1a fingerprint of a theme file, as `fnv1a64:<hex>`.
/// Not a security hash; used only to detect content changes.
pub fn fingerprint(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{h:016x}")
}

/// Everything needed to tell two theme files apart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThemeVersionInfo {
    pub id: String,
    pub version: ThemeVersion,
    pub schema_version: u32,
    pub fingerprint: String,
}

impl fmt::Display for ThemeVersionInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} (schema {}, {})",
            self.id, self.version, self.schema_version, self.fingerprint
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_orders_versions() {
        let a: ThemeVersion = "1.2.3".parse().unwrap();
        let b: ThemeVersion = "1.10.0".parse().unwrap();
        assert!(a < b, "numeric, not lexical, ordering");
        assert_eq!(a.to_string(), "1.2.3");
        for bad in ["1.2", "1.2.3.4", "01.2.3", "a.b.c", "", "1..3", "-1.0.0"] {
            assert!(bad.parse::<ThemeVersion>().is_err(), "{bad}");
        }
    }

    #[test]
    fn fingerprint_is_stable_and_content_sensitive() {
        // FNV-1a 64 reference vectors.
        assert_eq!(fingerprint(b""), "fnv1a64:cbf29ce484222325");
        assert_eq!(fingerprint(b"a"), "fnv1a64:af63dc4c8601ec8c");
        assert_ne!(fingerprint(b"theme"), fingerprint(b"theme "));
    }
}
