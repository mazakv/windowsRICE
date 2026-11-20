//! Captures a screen region (physical pixels) to PNG, optionally upscaled
//! with nearest-neighbour sampling for close inspection of glyphs.
//!
//! Usage: shot <out.png> <x> <y> <width> <height> [zoom]
//!
//! Read-only with respect to the system: it copies pixels from the screen
//! device context and writes only the output file.

use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 6 {
        eprintln!("usage: shot <out.png> <x> <y> <width> <height> [zoom]");
        std::process::exit(2);
    }
    let n = |i: usize| a[i].parse::<i32>().expect("integer argument");
    let (x, y, w, h) = (n(2), n(3), n(4), n(5));
    let zoom = a
        .get(6)
        .and_then(|z| z.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);

    // SAFETY: GDI screen capture into our own bitmap; all handles are released.
    let bgra = unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bmp.into());
        BitBlt(mem, 0, 0, w, h, Some(screen), x, y, SRCCOPY).expect("BitBlt");
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: u32::try_from(size_of::<BITMAPINFOHEADER>()).unwrap_or(0),
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        GetDIBits(
            mem,
            bmp,
            0,
            h as u32,
            Some(buf.as_mut_ptr().cast()),
            &raw mut info,
            DIB_RGB_COLORS,
        );
        SelectObject(mem, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        buf
    };

    let (w, h) = (w as usize, h as usize);
    let (zw, zh) = (w * zoom, h * zoom);
    let mut rgb = vec![0u8; zw * zh * 3];
    for zy in 0..zh {
        for zx in 0..zw {
            let s = ((zy / zoom) * w + (zx / zoom)) * 4;
            let d = (zy * zw + zx) * 3;
            rgb[d] = bgra[s + 2];
            rgb[d + 1] = bgra[s + 1];
            rgb[d + 2] = bgra[s];
        }
    }
    let file = std::fs::File::create(&a[1]).expect("create output");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), zw as u32, zh as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(&rgb))
        .expect("write png");
}
