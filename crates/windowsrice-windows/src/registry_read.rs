//! Read-only registry access.
//!
//! Only `RegGetValueW` is used. This module intentionally offers no write,
//! create or delete operations; registry changes will be implemented in the
//! Registry Manager (Phase 11) with journaling, verification and rollback.

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW,
};
use windows::core::HSTRING;

use crate::error::WinError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hive {
    CurrentUser,
    LocalMachine,
}

impl Hive {
    fn hkey(self) -> HKEY {
        match self {
            Hive::CurrentUser => HKEY_CURRENT_USER,
            Hive::LocalMachine => HKEY_LOCAL_MACHINE,
        }
    }
}

/// Reads a `REG_SZ` value. Returns `Ok(None)` if the key or value is absent.
pub fn read_string(hive: Hive, subkey: &str, value: &str) -> Result<Option<String>, WinError> {
    let subkey = HSTRING::from(subkey);
    let value = HSTRING::from(value);
    for _attempt in 0..3 {
        let mut size_bytes: u32 = 0;
        // SAFETY: valid HKEY constant and NUL-terminated strings; a null data
        // pointer with a size pointer queries the required size.
        let status = unsafe {
            RegGetValueW(
                hive.hkey(),
                &subkey,
                &value,
                RRF_RT_REG_SZ,
                None,
                None,
                Some(&mut size_bytes),
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            return Err(WinError::from_win32("RegGetValueW", status));
        }
        let mut buf = vec![0u16; (size_bytes as usize).div_ceil(2) + 1];
        let mut buf_bytes = u32::try_from(buf.len() * 2).unwrap_or(u32::MAX);
        // SAFETY: `buf` is writable for `buf_bytes` bytes.
        let status = unsafe {
            RegGetValueW(
                hive.hkey(),
                &subkey,
                &value,
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut buf_bytes),
            )
        };
        match status {
            s if s == ERROR_SUCCESS => return Ok(Some(crate::util::from_wide(&buf))),
            s if s == ERROR_FILE_NOT_FOUND => return Ok(None),
            s if s == ERROR_MORE_DATA => continue, // value grew between calls
            s => return Err(WinError::from_win32("RegGetValueW", s)),
        }
    }
    Err(WinError::Unavailable(
        "registry value kept changing size while being read".into(),
    ))
}

/// Reads a `REG_DWORD` value. Returns `Ok(None)` if the key or value is absent.
pub fn read_dword(hive: Hive, subkey: &str, value: &str) -> Result<Option<u32>, WinError> {
    let subkey = HSTRING::from(subkey);
    let value = HSTRING::from(value);
    let mut data: u32 = 0;
    let mut size = u32::try_from(size_of::<u32>()).unwrap_or(4);
    // SAFETY: `data` is a writable u32 and `size` matches it.
    let status = unsafe {
        RegGetValueW(
            hive.hkey(),
            &subkey,
            &value,
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut data).cast()),
            Some(&mut size),
        )
    };
    match status {
        s if s == ERROR_SUCCESS => Ok(Some(data)),
        s if s == ERROR_FILE_NOT_FOUND => Ok(None),
        s => Err(WinError::from_win32("RegGetValueW", s)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CURRENT_VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

    /// On-machine test: reads values every Windows installation has.
    #[test]
    fn live_reads_existing_values() {
        let build = read_string(Hive::LocalMachine, CURRENT_VERSION, "CurrentBuild").unwrap();
        assert!(build.is_some_and(|b| b.parse::<u32>().is_ok()));
        let major = read_dword(
            Hive::LocalMachine,
            CURRENT_VERSION,
            "CurrentMajorVersionNumber",
        )
        .unwrap();
        assert_eq!(major, Some(10));
    }

    #[test]
    fn live_missing_values_are_none() {
        assert_eq!(
            read_string(
                Hive::CurrentUser,
                r"Software\windowsRICE-nonexistent-test-key",
                "x"
            )
            .unwrap(),
            None
        );
        assert_eq!(
            read_dword(
                Hive::LocalMachine,
                CURRENT_VERSION,
                "windowsRICE-nonexistent"
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn live_type_mismatch_is_an_error_not_a_value() {
        // CurrentBuild is REG_SZ, so a DWORD read must fail rather than return garbage.
        assert!(read_dword(Hive::LocalMachine, CURRENT_VERSION, "CurrentBuild").is_err());
    }
}
