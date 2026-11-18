/// Converts a NUL-terminated (or NUL-padded) UTF-16 buffer to a `String`.
pub(crate) fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// Like [`from_wide`], returning `None` for empty or whitespace-only text.
pub(crate) fn from_wide_nonempty(buf: &[u16]) -> Option<String> {
    let s = from_wide(buf);
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops_at_nul_and_handles_korean() {
        let mut v: Vec<u16> = "모니터 A".encode_utf16().collect();
        v.extend([0, 65, 66]);
        assert_eq!(from_wide(&v), "모니터 A");
        assert_eq!(from_wide_nonempty(&[0, 0]), None);
        assert_eq!(from_wide_nonempty(&[32, 65, 32, 0]), Some("A".into()));
    }
}
