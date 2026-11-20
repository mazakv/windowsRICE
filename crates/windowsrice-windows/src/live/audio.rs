//! Default audio output volume (read-only).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, EDataFlow, ERole, IMMDeviceEnumerator,
    IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator, eConsole, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::core::{PCWSTR, implement};

use crate::error::{WinError, api};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputVolume {
    pub percent: u8,
    pub muted: bool,
}

/// `HRESULT_FROM_WIN32(ERROR_NOT_FOUND)`: there is no default endpoint.
const E_NOTFOUND: i32 = 0x8007_0490_u32.cast_signed();

/// Volume and mute state of the default render endpoint, or `None` when
/// there is no audio output device. Requires COM on the calling thread.
pub fn default_output() -> Result<Option<OutputVolume>, WinError> {
    // SAFETY: COM calls on interfaces we own; no out-pointers.
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(api("CoCreateInstance(MMDeviceEnumerator)"))?;
        let device = match enumerator.GetDefaultAudioEndpoint(eRender, eConsole) {
            Ok(d) => d,
            Err(e) if e.code().0 == E_NOTFOUND => return Ok(None),
            Err(e) => return Err(WinError::from_windows("GetDefaultAudioEndpoint", &e)),
        };
        let volume: IAudioEndpointVolume = device
            .Activate(CLSCTX_ALL, None)
            .map_err(api("IMMDevice::Activate"))?;
        let level = volume
            .GetMasterVolumeLevelScalar()
            .map_err(api("GetMasterVolumeLevelScalar"))?;
        let muted = volume.GetMute().map_err(api("GetMute"))?.as_bool();
        Ok(Some(OutputVolume {
            percent: (level.clamp(0.0, 1.0) * 100.0).round() as u8,
            muted,
        }))
    }
}

/// Wakes the caller when the default output device or its volume changes.
/// Callbacks arrive on COM worker threads and only call `signal`; the
/// owner re-reads with [`AudioWatcher::read`], which also re-binds to a new
/// default device.
pub struct AudioWatcher {
    enumerator: IMMDeviceEnumerator,
    client: IMMNotificationClient,
    bound: Option<(IAudioEndpointVolume, IAudioEndpointVolumeCallback)>,
    device_changed: Arc<AtomicBool>,
    signal: Arc<dyn Fn() + Send + Sync>,
}

impl std::fmt::Debug for AudioWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioWatcher")
            .field("bound", &self.bound.is_some())
            .finish_non_exhaustive()
    }
}

#[implement(IMMNotificationClient)]
struct DeviceEvents {
    changed: Arc<AtomicBool>,
    signal: Arc<dyn Fn() + Send + Sync>,
}

impl IMMNotificationClient_Impl for DeviceEvents_Impl {
    fn OnDeviceStateChanged(
        &self,
        _id: &PCWSTR,
        _state: DEVICE_STATE,
    ) -> windows::core::Result<()> {
        self.changed.store(true, Ordering::SeqCst);
        (self.signal)();
        Ok(())
    }
    fn OnDeviceAdded(&self, _id: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnDeviceRemoved(&self, _id: &PCWSTR) -> windows::core::Result<()> {
        self.changed.store(true, Ordering::SeqCst);
        (self.signal)();
        Ok(())
    }
    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        _id: &PCWSTR,
    ) -> windows::core::Result<()> {
        if flow == eRender && role == eConsole {
            self.changed.store(true, Ordering::SeqCst);
            (self.signal)();
        }
        Ok(())
    }
    fn OnPropertyValueChanged(
        &self,
        _id: &PCWSTR,
        _key: &PROPERTYKEY,
    ) -> windows::core::Result<()> {
        Ok(())
    }
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeEvents {
    signal: Arc<dyn Fn() + Send + Sync>,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeEvents_Impl {
    fn OnNotify(&self, _data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        (self.signal)();
        Ok(())
    }
}

impl AudioWatcher {
    /// Requires COM (multithreaded) on the calling thread.
    pub fn new(signal: Arc<dyn Fn() + Send + Sync>) -> Result<Self, WinError> {
        let changed = Arc::new(AtomicBool::new(true));
        // SAFETY: COM calls on interfaces we own.
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .map_err(api("CoCreateInstance(MMDeviceEnumerator)"))?;
            let client: IMMNotificationClient = DeviceEvents {
                changed: changed.clone(),
                signal: signal.clone(),
            }
            .into();
            enumerator
                .RegisterEndpointNotificationCallback(&client)
                .map_err(api("RegisterEndpointNotificationCallback"))?;
            Ok(Self {
                enumerator,
                client,
                bound: None,
                device_changed: changed,
                signal,
            })
        }
    }

    fn unbind(&mut self) {
        if let Some((volume, cb)) = self.bound.take() {
            // SAFETY: unregistering the callback registered in `read`.
            let _ = unsafe { volume.UnregisterControlChangeNotify(&cb) };
        }
    }

    /// Current state of the default output, or `None` without a device.
    pub fn read(&mut self) -> Result<Option<OutputVolume>, WinError> {
        if self.device_changed.swap(false, Ordering::SeqCst) || self.bound.is_none() {
            self.unbind();
            // SAFETY: COM calls on interfaces we own.
            unsafe {
                let device = match self.enumerator.GetDefaultAudioEndpoint(eRender, eConsole) {
                    Ok(d) => d,
                    Err(e) if e.code().0 == E_NOTFOUND => return Ok(None),
                    Err(e) => return Err(WinError::from_windows("GetDefaultAudioEndpoint", &e)),
                };
                let volume: IAudioEndpointVolume = device
                    .Activate(CLSCTX_ALL, None)
                    .map_err(api("IMMDevice::Activate"))?;
                let cb: IAudioEndpointVolumeCallback = VolumeEvents {
                    signal: self.signal.clone(),
                }
                .into();
                volume
                    .RegisterControlChangeNotify(&cb)
                    .map_err(api("RegisterControlChangeNotify"))?;
                self.bound = Some((volume, cb));
            }
        }
        let Some((volume, _)) = &self.bound else {
            return Ok(None);
        };
        // SAFETY: COM calls on our bound endpoint.
        unsafe {
            let level = volume
                .GetMasterVolumeLevelScalar()
                .map_err(api("GetMasterVolumeLevelScalar"))?;
            let muted = volume.GetMute().map_err(api("GetMute"))?.as_bool();
            Ok(Some(OutputVolume {
                percent: (level.clamp(0.0, 1.0) * 100.0).round() as u8,
                muted,
            }))
        }
    }
}

impl Drop for AudioWatcher {
    fn drop(&mut self) {
        self.unbind();
        // SAFETY: unregistering the client registered in `new`.
        let _ = unsafe {
            self.enumerator
                .UnregisterEndpointNotificationCallback(&self.client)
        };
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

    /// Real Windows: the watcher binds to the default device and reads it.
    #[test]
    fn watcher_reads_default_output() {
        // SAFETY: initialises COM for this test thread only.
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let mut w = AudioWatcher::new(Arc::new(|| {})).unwrap();
        let a = w.read().unwrap();
        assert_eq!(a, default_output().unwrap());
    }

    /// Real Windows: reads the default output device (read-only).
    #[test]
    fn reads_default_output() {
        // SAFETY: initialises COM for this test thread only.
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if let Some(v) = default_output().unwrap() {
            assert!(v.percent <= 100);
        }
    }
}
