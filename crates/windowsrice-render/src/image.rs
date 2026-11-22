//! Background images and video poster frames (Phase 14).
//!
//! Files come from the design library only (copied there after the checks
//! in `windowsrice_core::design_pack`). PNG and JPEG are decoded with WIC;
//! for an MP4 video, Media Foundation's source reader decodes the single
//! frame at the requested time (a still poster frame; continuous video
//! playback is not implemented). Bitmaps are cached per device, so a frame
//! is decoded once.

use windows::Win32::Foundation::GENERIC_READ;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_SIZE_U, D2D1_ALPHA_MODE_IGNORE, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_OPTIONS_NONE, D2D1_BITMAP_PROPERTIES1, ID2D1Bitmap1, ID2D1DeviceContext,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory,
    WICBitmapDitherTypeNone, WICBitmapPaletteTypeMedianCut, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFSourceReader, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
    MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_VERSION,
    MFCreateAttributes, MFCreateMediaType, MFCreateSourceReaderFromURL, MFMediaType_Video,
    MFSTARTUP_LITE, MFStartup, MFVideoFormat_RGB32,
};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::core::{GUID, HSTRING, Interface};

/// Decodes `path` (frame `frame_ms` for a video) into a device bitmap.
pub(crate) fn load(
    dc: &ID2D1DeviceContext,
    path: &str,
    frame_ms: u32,
) -> windows::core::Result<ID2D1Bitmap1> {
    if path.to_ascii_lowercase().ends_with(".mp4") {
        video_frame(dc, path, frame_ms)
    } else {
        image(dc, path)
    }
}

fn image(dc: &ID2D1DeviceContext, path: &str) -> windows::core::Result<ID2D1Bitmap1> {
    // SAFETY: WIC decode of a local file into a converter we own, then a
    // device bitmap created from it; all interfaces are released on drop.
    unsafe {
        let wic: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let dec = wic.CreateDecoderFromFilename(
            &HSTRING::from(path),
            None,
            GENERIC_READ,
            WICDecodeMetadataCacheOnDemand,
        )?;
        let frame = dec.GetFrame(0)?;
        let conv = wic.CreateFormatConverter()?;
        conv.Initialize(
            &frame,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeMedianCut,
        )?;
        let props = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: D2D1_BITMAP_OPTIONS_NONE,
            colorContext: std::mem::ManuallyDrop::new(None),
        };
        dc.CreateBitmapFromWicBitmap(&conv, Some(&raw const props))
    }
}

fn video_frame(
    dc: &ID2D1DeviceContext,
    path: &str,
    frame_ms: u32,
) -> windows::core::Result<ID2D1Bitmap1> {
    // SAFETY: Media Foundation on this thread (MFStartup is reference
    // counted and cheap after the first call); a source reader we own
    // decodes one frame into a buffer that is locked only while copied.
    unsafe {
        MFStartup(MF_VERSION, MFSTARTUP_LITE)?;
        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&raw mut attrs, 1)?;
        let attrs =
            attrs.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;
        // Lets the reader convert to RGB32 itself.
        attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)?;
        let reader: IMFSourceReader = MFCreateSourceReaderFromURL(&HSTRING::from(path), &attrs)?;
        let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let mt = MFCreateMediaType()?;
        mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        mt.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
        reader.SetCurrentMediaType(stream, None, &mt)?;
        let cur = reader.GetCurrentMediaType(stream)?;
        let packed = cur.GetUINT64(&MF_MT_FRAME_SIZE)?;
        let (w, h) = ((packed >> 32) as u32, (packed & 0xFFFF_FFFF) as u32);
        if w == 0 || h == 0 || w > 3840 || h > 2160 {
            return Err(windows::Win32::Foundation::E_INVALIDARG.into());
        }
        if frame_ms > 0 {
            let pos = PROPVARIANT::from(i64::from(frame_ms) * 10_000);
            reader.SetCurrentPosition(&GUID::zeroed(), &raw const pos)?;
        }
        // The first sample at or after the position (a few reads at most).
        for _ in 0..120 {
            let mut flags = 0u32;
            let mut sample = None;
            reader.ReadSample(
                stream,
                0,
                None,
                Some(&raw mut flags),
                None,
                Some(&raw mut sample),
            )?;
            if flags & 0x2 != 0 {
                // End of stream before the position.
                break;
            }
            let Some(sample) = sample else { continue };
            let buf = sample.ConvertToContiguousBuffer()?;
            let mut p = std::ptr::null_mut();
            let mut len = 0u32;
            buf.Lock(&raw mut p, None, Some(&raw mut len))?;
            let stride = w * 4;
            let ok = (len as usize) >= (stride * h) as usize;
            let bmp = if ok {
                let props = D2D1_BITMAP_PROPERTIES1 {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_IGNORE,
                    },
                    dpiX: 96.0,
                    dpiY: 96.0,
                    bitmapOptions: D2D1_BITMAP_OPTIONS_NONE,
                    colorContext: std::mem::ManuallyDrop::new(None),
                };
                dc.CreateBitmap(
                    D2D_SIZE_U {
                        width: w,
                        height: h,
                    },
                    Some(p.cast_const().cast()),
                    stride,
                    &raw const props,
                )
            } else {
                Err(windows::Win32::Foundation::E_UNEXPECTED.into())
            };
            let _ = buf.Unlock();
            return bmp;
        }
        let _ = IMFSourceReader::IID;
        Err(windows::Win32::Foundation::E_FAIL.into())
    }
}
