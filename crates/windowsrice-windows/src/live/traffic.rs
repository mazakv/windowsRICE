//! Network throughput from interface byte counters (read-only).

use std::time::Instant;

use windows::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;

use crate::error::WinError;

const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;
const IF_TYPE_TUNNEL: u32 = 131;

/// Total received and sent bytes over physical interfaces that are up.
pub fn interface_octets() -> Result<(u64, u64), WinError> {
    let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
    // SAFETY: GetIfTable2 allocates the table; it is freed with FreeMibTable
    // below and only read in between.
    unsafe {
        let r = GetIfTable2(&raw mut table);
        if r.is_err() || table.is_null() {
            return Err(WinError::from_win32("GetIfTable2", r));
        }
        let n = (*table).NumEntries as usize;
        let rows = std::slice::from_raw_parts((*table).Table.as_ptr(), n);
        let (mut rx, mut tx) = (0u64, 0u64);
        for row in rows {
            let hardware = row.InterfaceAndOperStatusFlags._bitfield & 1 != 0;
            if hardware
                && row.OperStatus == IfOperStatusUp
                && row.Type != IF_TYPE_SOFTWARE_LOOPBACK
                && row.Type != IF_TYPE_TUNNEL
            {
                rx = rx.saturating_add(row.InOctets);
                tx = tx.saturating_add(row.OutOctets);
            }
        }
        FreeMibTable(table.cast());
        Ok((rx, tx))
    }
}

/// Upload and download rates from successive counter readings.
#[derive(Debug, Default)]
pub struct TrafficSampler {
    prev: Option<(Instant, u64, u64)>,
}

impl TrafficSampler {
    pub fn new() -> Self {
        Self::default()
    }

    /// `(upload, download)` in bytes per second since the previous call; the
    /// first call primes the sampler and returns `None`. A counter that went
    /// backwards (interface reset) also yields `None` for that interval.
    pub fn sample(&mut self) -> Result<Option<(f64, f64)>, WinError> {
        let (rx, tx) = interface_octets()?;
        let now = Instant::now();
        let out = self.prev.and_then(|(t, prx, ptx)| {
            let dt = now.duration_since(t).as_secs_f64();
            let drx = rx.checked_sub(prx)?;
            let dtx = tx.checked_sub(ptx)?;
            (dt > 0.0).then(|| (dtx as f64 / dt, drx as f64 / dt))
        });
        self.prev = Some((now, rx, tx));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real Windows: counters are readable and rates are non-negative.
    #[test]
    fn reads_counters() {
        let mut s = TrafficSampler::new();
        assert_eq!(s.sample().unwrap(), None);
        std::thread::sleep(std::time::Duration::from_millis(100));
        if let Some((up, down)) = s.sample().unwrap() {
            assert!(up >= 0.0 && down >= 0.0);
        }
    }
}
