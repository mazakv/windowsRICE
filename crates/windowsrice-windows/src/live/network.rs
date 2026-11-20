//! Network connectivity (read-only).

use std::sync::Arc;
use windows::Win32::NetworkManagement::IpHelper::{GetBestInterface, GetIfEntry2, MIB_IF_ROW2};
use windows::Win32::Networking::NetworkListManager::{
    INetworkListManager, INetworkListManagerEvents, INetworkListManagerEvents_Impl,
    NLM_CONNECTIVITY, NLM_CONNECTIVITY_IPV4_INTERNET, NLM_CONNECTIVITY_IPV4_LOCALNETWORK,
    NLM_CONNECTIVITY_IPV4_SUBNET, NLM_CONNECTIVITY_IPV6_INTERNET,
    NLM_CONNECTIVITY_IPV6_LOCALNETWORK, NLM_CONNECTIVITY_IPV6_SUBNET, NetworkListManager,
};

use windows::Win32::System::Com::{
    CLSCTX_ALL, CoCreateInstance, IConnectionPoint, IConnectionPointContainer,
};
use windows::core::{Interface, implement};

use crate::error::{WinError, api};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connectivity {
    Internet,
    LocalOnly,
    Disconnected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkType {
    Wired,
    Wireless,
    Other,
}

const IF_TYPE_ETHERNET_CSMACD: u32 = 6;
const IF_TYPE_IEEE80211: u32 = 71;

/// Connectivity as reported by the Network List Manager. Requires COM on
/// the calling thread.
pub fn connectivity() -> Result<Connectivity, WinError> {
    // SAFETY: COM call on an interface we own.
    let flags = unsafe {
        let nlm: INetworkListManager = CoCreateInstance(&NetworkListManager, None, CLSCTX_ALL)
            .map_err(api("CoCreateInstance(NetworkListManager)"))?;
        nlm.GetConnectivity().map_err(api("GetConnectivity"))?
    };
    let has =
        |f: windows::Win32::Networking::NetworkListManager::NLM_CONNECTIVITY| flags.0 & f.0 != 0;
    Ok(
        if has(NLM_CONNECTIVITY_IPV4_INTERNET) || has(NLM_CONNECTIVITY_IPV6_INTERNET) {
            Connectivity::Internet
        } else if has(NLM_CONNECTIVITY_IPV4_LOCALNETWORK)
            || has(NLM_CONNECTIVITY_IPV6_LOCALNETWORK)
            || has(NLM_CONNECTIVITY_IPV4_SUBNET)
            || has(NLM_CONNECTIVITY_IPV6_SUBNET)
        {
            Connectivity::LocalOnly
        } else {
            Connectivity::Disconnected
        },
    )
}

/// Type of the interface Windows would use to reach the internet. This is a
/// routing-table lookup only; no packets are sent.
pub fn primary_link() -> Option<LinkType> {
    let dest = u32::from_ne_bytes([8, 8, 8, 8]);
    let mut index = 0u32;
    // SAFETY: plain out-parameter.
    if unsafe { GetBestInterface(dest, &raw mut index) } != 0 {
        return None;
    }
    let mut row = MIB_IF_ROW2 {
        InterfaceIndex: index,
        ..Default::default()
    };
    // SAFETY: `row` is a valid MIB_IF_ROW2 with its index set.
    if unsafe { GetIfEntry2(&raw mut row) }.is_err() {
        return None;
    }
    Some(match row.Type {
        IF_TYPE_IEEE80211 => LinkType::Wireless,
        IF_TYPE_ETHERNET_CSMACD => LinkType::Wired,
        _ => LinkType::Other,
    })
}

/// Calls `signal` when Windows reports a connectivity change (Network List
/// Manager events, delivered on a COM worker thread).
pub struct ConnectivityWatcher {
    point: IConnectionPoint,
    cookie: u32,
}

impl std::fmt::Debug for ConnectivityWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectivityWatcher")
            .field("cookie", &self.cookie)
            .finish()
    }
}

#[implement(INetworkListManagerEvents)]
struct NetEvents {
    signal: Arc<dyn Fn() + Send + Sync>,
}

impl INetworkListManagerEvents_Impl for NetEvents_Impl {
    fn ConnectivityChanged(&self, _c: NLM_CONNECTIVITY) -> windows::core::Result<()> {
        (self.signal)();
        Ok(())
    }
}

impl ConnectivityWatcher {
    /// Requires COM (multithreaded) on the calling thread.
    pub fn new(signal: Arc<dyn Fn() + Send + Sync>) -> Result<Self, WinError> {
        // SAFETY: COM calls on interfaces we own; the advise cookie is
        // released in Drop.
        unsafe {
            let nlm: INetworkListManager = CoCreateInstance(&NetworkListManager, None, CLSCTX_ALL)
                .map_err(api("CoCreateInstance(NetworkListManager)"))?;
            let container: IConnectionPointContainer =
                nlm.cast().map_err(api("IConnectionPointContainer"))?;
            let point = container
                .FindConnectionPoint(&INetworkListManagerEvents::IID)
                .map_err(api("FindConnectionPoint"))?;
            let sink: INetworkListManagerEvents = NetEvents { signal }.into();
            let cookie = point
                .Advise(&sink)
                .map_err(api("IConnectionPoint::Advise"))?;
            Ok(Self { point, cookie })
        }
    }
}

impl Drop for ConnectivityWatcher {
    fn drop(&mut self) {
        // SAFETY: releasing the subscription made in `new`.
        let _ = unsafe { self.point.Unadvise(self.cookie) };
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

    /// Real Windows: subscribing and unsubscribing to connectivity events.
    #[test]
    fn watcher_subscribes() {
        // SAFETY: initialises COM for this test thread only.
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let w = ConnectivityWatcher::new(Arc::new(|| {})).unwrap();
        drop(w);
    }

    /// Real Windows: queries connectivity and the routing table (read-only).
    #[test]
    fn reads_connectivity() {
        // SAFETY: initialises COM for this test thread only.
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let c = connectivity().unwrap();
        if c == Connectivity::Internet {
            assert!(primary_link().is_some());
        }
    }
}
