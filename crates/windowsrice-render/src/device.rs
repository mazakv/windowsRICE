//! Device, swap chain, composition and painting.

use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_GRADIENT_STOP, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_ALIASED, D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_TARGET,
    D2D1_BITMAP_PROPERTIES1, D2D1_BUFFER_PRECISION_8BPC_UNORM,
    D2D1_COLOR_INTERPOLATION_MODE_STRAIGHT, D2D1_COLOR_SPACE_SRGB,
    D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_EXTEND_MODE_CLAMP,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES, D2D1_ROUNDED_RECT,
    D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateFactory, ID2D1Bitmap1, ID2D1DeviceContext,
    ID2D1Factory1, ID2D1GradientStopCollection, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP, D3D_FEATURE_LEVEL,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionEffectGroup, IDCompositionTarget,
    IDCompositionVisual,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_UNKNOWN,
    DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, DXGI_ADAPTER_DESC, DXGI_CREATE_FACTORY_FLAGS, DXGI_PRESENT,
    DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG,
    DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIDevice, IDXGIDevice3,
    IDXGIFactory2, IDXGISurface, IDXGISwapChain1,
};
use windows::core::Interface;
use windows_numerics::Vector2;
use windowsrice_theme::Color;
use windowsrice_ui::draw::{DrawCmd, DrawList};

use crate::error::{RenderError, classify};
use crate::text::TextSystem;

/// What the renderer is running on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderInfo {
    pub adapter: String,
    /// `0xB000` for 11.0, etc.
    pub feature_level: u32,
    pub warp: bool,
    pub width: u32,
    pub height: u32,
    pub dpi: u32,
    /// Time to create the device and surface.
    pub init: Duration,
    pub recoveries: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameStats {
    /// Drawing (BeginDraw → EndDraw).
    pub draw: Duration,
    /// `Present` call (blocks until the frame is queued).
    pub present: Duration,
}

/// Device-dependent objects, rebuilt as a unit on device loss.
struct Gpu {
    d3d: ID3D11Device,
    dxgi: IDXGIDevice,
    dc: ID2D1DeviceContext,
    brush: ID2D1SolidColorBrush,
    swapchain: IDXGISwapChain1,
    target: Option<ID2D1Bitmap1>,
    comp: IDCompositionDevice,
    _comp_target: IDCompositionTarget,
    _visual: IDCompositionVisual,
    effect: IDCompositionEffectGroup,
    adapter: String,
    feature_level: u32,
    warp: bool,
    /// Decoded background images and video frames (Phase 14), by file and
    /// frame; `None` remembers a file that could not be decoded.
    images: std::collections::HashMap<
        (String, u32),
        Option<windows::Win32::Graphics::Direct2D::ID2D1Bitmap1>,
    >,
}

pub struct Renderer {
    hwnd: HWND,
    gpu: Option<Gpu>,
    text: TextSystem,
    width: u32,
    height: u32,
    dpi: u32,
    init: Duration,
    recoveries: u32,
    simulate_loss: bool,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("dpi", &self.dpi)
            .field("has_device", &self.gpu.is_some())
            .finish_non_exhaustive()
    }
}

fn d2d_color(c: Color) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: f32::from(c.r) / 255.0,
        g: f32::from(c.g) / 255.0,
        b: f32::from(c.b) / 255.0,
        a: f32::from(c.a) / 255.0,
    }
}

fn create_d3d(driver: D3D_DRIVER_TYPE) -> windows::core::Result<(ID3D11Device, D3D_FEATURE_LEVEL)> {
    let mut device = None;
    let mut level = D3D_FEATURE_LEVEL::default();
    // SAFETY: valid out-pointers; no software module.
    unsafe {
        D3D11CreateDevice(
            None,
            driver,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&raw mut device),
            Some(&raw mut level),
            None,
        )?;
    }
    Ok((device.ok_or_else(windows::core::Error::empty)?, level))
}

impl Gpu {
    fn new(hwnd: HWND, width: u32, height: u32, dpi: u32) -> windows::core::Result<Self> {
        let (d3d, level, warp) = match create_d3d(D3D_DRIVER_TYPE_HARDWARE) {
            Ok((d, l)) => (d, l, false),
            Err(_) => {
                let (d, l) = create_d3d(D3D_DRIVER_TYPE_WARP)?;
                (d, l, true)
            }
        };
        // SAFETY: standard DXGI/D2D/DComp initialisation; every object created
        // here is owned by the returned `Gpu`.
        unsafe {
            let dxgi: IDXGIDevice = d3d.cast()?;
            let desc: DXGI_ADAPTER_DESC = dxgi.GetAdapter()?.GetDesc()?;
            let n = desc.Description.iter().position(|&c| c == 0).unwrap_or(128);
            let adapter = String::from_utf16_lossy(&desc.Description[..n]);

            let factory: IDXGIFactory2 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))?;
            let sc_desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: width.max(1),
                Height: height.max(1),
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
                ..Default::default()
            };
            let swapchain = factory.CreateSwapChainForComposition(&d3d, &sc_desc, None)?;

            let d2d: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dc = d2d
                .CreateDevice(&dxgi)?
                .CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            dc.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let brush = dc.CreateSolidColorBrush(&d2d_color(Color::WHITE), None)?;

            let comp: IDCompositionDevice = DCompositionCreateDevice(&dxgi)?;
            let comp_target = comp.CreateTargetForHwnd(hwnd, true)?;
            let visual = comp.CreateVisual()?;
            let effect = comp.CreateEffectGroup()?;
            visual.SetContent(&swapchain)?;
            visual.SetEffect(&effect)?;
            comp_target.SetRoot(&visual)?;
            comp.Commit()?;

            let mut gpu = Gpu {
                d3d,
                dxgi,
                dc,
                brush,
                swapchain,
                target: None,
                comp,
                _comp_target: comp_target,
                _visual: visual,
                effect,
                adapter,
                feature_level: level.0.cast_unsigned(),
                warp,
                images: std::collections::HashMap::new(),
            };
            gpu.bind_target(dpi)?;
            Ok(gpu)
        }
    }

    fn bind_target(&mut self, dpi: u32) -> windows::core::Result<()> {
        // SAFETY: the back buffer belongs to our swap chain; the bitmap's DPI
        // makes the device context draw in DIPs.
        unsafe {
            let surface: IDXGISurface = self.swapchain.GetBuffer(0)?;
            let props = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: dpi as f32,
                dpiY: dpi as f32,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                ..Default::default()
            };
            let bmp = self
                .dc
                .CreateBitmapFromDxgiSurface(&surface, Some(&raw const props))?;
            self.dc.SetTarget(&bmp);
            // The context's DPI is independent of the target bitmap's DPI and
            // must be set explicitly, or drawing stays at 96 DPI (1 DIP = 1 px).
            self.dc.SetDpi(dpi as f32, dpi as f32);
            self.target = Some(bmp);
        }
        Ok(())
    }
}

impl Renderer {
    /// Creates the device and the composition surface for `hwnd`, which must
    /// have been created with `WS_EX_NOREDIRECTIONBITMAP`.
    pub fn new(
        hwnd: HWND,
        width: u32,
        height: u32,
        dpi: u32,
        text: TextSystem,
    ) -> Result<Self, RenderError> {
        let t = Instant::now();
        let gpu = Gpu::new(hwnd, width, height, dpi).map_err(classify)?;
        Ok(Self {
            hwnd,
            gpu: Some(gpu),
            text,
            width,
            height,
            dpi,
            init: t.elapsed(),
            recoveries: 0,
            simulate_loss: false,
        })
    }

    pub fn text(&self) -> &TextSystem {
        &self.text
    }

    /// Replaces the text system (theme fonts or language changed).
    pub fn set_text(&mut self, text: TextSystem) {
        self.text = text;
    }

    pub fn info(&self) -> RenderInfo {
        let (adapter, feature_level, warp) =
            self.gpu.as_ref().map_or((String::new(), 0, false), |g| {
                (g.adapter.clone(), g.feature_level, g.warp)
            });
        RenderInfo {
            adapter,
            feature_level,
            warp,
            width: self.width,
            height: self.height,
            dpi: self.dpi,
            init: self.init,
            recoveries: self.recoveries,
        }
    }

    /// Resizes the swap chain and rebinds the target at `dpi`.
    pub fn resize(&mut self, width: u32, height: u32, dpi: u32) -> Result<(), RenderError> {
        self.width = width;
        self.height = height;
        self.dpi = dpi;
        let Some(gpu) = self.gpu.as_mut() else {
            return Ok(());
        };
        // SAFETY: the target is released before the buffers are resized.
        unsafe {
            gpu.dc.SetTarget(None);
            gpu.target = None;
            gpu.swapchain
                .ResizeBuffers(
                    0,
                    width.max(1),
                    height.max(1),
                    DXGI_FORMAT_UNKNOWN,
                    DXGI_SWAP_CHAIN_FLAG(0),
                )
                .map_err(classify)?;
        }
        gpu.bind_target(dpi).map_err(classify)
    }

    /// Test hook: the next [`Renderer::render`] reports device loss, which
    /// drives the same recovery path as a real device removal.
    pub fn simulate_device_loss(&mut self) {
        self.simulate_loss = true;
    }

    /// Releases every device-dependent object and creates them again.
    pub fn recover(&mut self) -> Result<(), RenderError> {
        // The composition target must be released before a new one can be
        // created for the same window.
        self.gpu = None;
        let gpu = Gpu::new(self.hwnd, self.width, self.height, self.dpi).map_err(classify)?;
        self.gpu = Some(gpu);
        self.recoveries += 1;
        Ok(())
    }

    /// Draws `list` and presents it.
    pub fn render(&mut self, list: &DrawList) -> Result<FrameStats, RenderError> {
        if std::mem::take(&mut self.simulate_loss) {
            return Err(RenderError::DeviceLost("simulated".into()));
        }
        let Some(gpu) = self.gpu.as_mut() else {
            return Err(RenderError::DeviceLost("no device".into()));
        };
        let t0 = Instant::now();
        // SAFETY: drawing on our own device context between BeginDraw and
        // EndDraw; all COM objects are owned by `gpu` or created here.
        unsafe {
            gpu.dc.BeginDraw();
            let mut clips = 0u32;
            gpu.dc.Clear(Some(&D2D1_COLOR_F::default()));
            for cmd in &list.cmds {
                match cmd {
                    DrawCmd::Fill {
                        rect,
                        radius,
                        color,
                        ..
                    } => {
                        gpu.brush.SetColor(&d2d_color(*color));
                        let r = to_d2d(*rect);
                        if *radius > 0.0 {
                            let rr = D2D1_ROUNDED_RECT {
                                rect: r,
                                radiusX: *radius,
                                radiusY: *radius,
                            };
                            gpu.dc.FillRoundedRectangle(&raw const rr, &gpu.brush);
                        } else {
                            gpu.dc.FillRectangle(&raw const r, &gpu.brush);
                        }
                    }
                    DrawCmd::Stroke {
                        rect,
                        radius,
                        width,
                        color,
                        ..
                    } => {
                        gpu.brush.SetColor(&d2d_color(*color));
                        let rr = D2D1_ROUNDED_RECT {
                            rect: to_d2d(*rect),
                            radiusX: *radius,
                            radiusY: *radius,
                        };
                        gpu.dc
                            .DrawRoundedRectangle(&raw const rr, &gpu.brush, *width, None);
                    }
                    DrawCmd::Text {
                        rect,
                        text,
                        style,
                        color,
                        trim,
                        ..
                    } => {
                        let layout = self
                            .text
                            .layout(text, style, rect.w + 1.0, rect.h, *trim)
                            .map_err(classify)?;
                        gpu.brush.SetColor(&d2d_color(*color));
                        gpu.dc.DrawTextLayout(
                            Vector2 {
                                X: rect.x,
                                Y: rect.y,
                            },
                            &layout,
                            &gpu.brush,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                        );
                    }
                    DrawCmd::Gradient {
                        rect,
                        radius,
                        from,
                        to,
                        stops,
                        ..
                    } => {
                        let gs: Vec<D2D1_GRADIENT_STOP> = stops
                            .iter()
                            .map(|(p, c)| D2D1_GRADIENT_STOP {
                                position: *p,
                                color: d2d_color(*c),
                            })
                            .collect();
                        let coll = gpu
                            .dc
                            .CreateGradientStopCollection(
                                &gs,
                                D2D1_COLOR_SPACE_SRGB,
                                D2D1_COLOR_SPACE_SRGB,
                                D2D1_BUFFER_PRECISION_8BPC_UNORM,
                                D2D1_EXTEND_MODE_CLAMP,
                                D2D1_COLOR_INTERPOLATION_MODE_STRAIGHT,
                            )
                            .and_then(|c| c.cast::<ID2D1GradientStopCollection>())
                            .map_err(classify)?;
                        let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                            startPoint: Vector2 {
                                X: rect.x + from.0 * rect.w,
                                Y: rect.y + from.1 * rect.h,
                            },
                            endPoint: Vector2 {
                                X: rect.x + to.0 * rect.w,
                                Y: rect.y + to.1 * rect.h,
                            },
                        };
                        let brush = gpu
                            .dc
                            .CreateLinearGradientBrush(&raw const props, None, &coll)
                            .map_err(classify)?;
                        let r = to_d2d(*rect);
                        if *radius > 0.0 {
                            let rr = D2D1_ROUNDED_RECT {
                                rect: r,
                                radiusX: *radius,
                                radiusY: *radius,
                            };
                            gpu.dc.FillRoundedRectangle(&raw const rr, &brush);
                        } else {
                            gpu.dc.FillRectangle(&raw const r, &brush);
                        }
                    }
                    DrawCmd::Wrapped {
                        rect,
                        text,
                        style,
                        color,
                        ..
                    } => {
                        let layout = self
                            .text
                            .layout_wrapped(text, style, rect.w, rect.h)
                            .map_err(classify)?;
                        gpu.brush.SetColor(&d2d_color(*color));
                        gpu.dc.DrawTextLayout(
                            Vector2 {
                                X: rect.x,
                                Y: rect.y,
                            },
                            &layout,
                            &gpu.brush,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                        );
                    }
                    DrawCmd::Image {
                        rect,
                        path,
                        frame_ms,
                        opacity,
                        ..
                    } => {
                        let key = (path.clone(), *frame_ms);
                        if !gpu.images.contains_key(&key) {
                            let b = crate::image::load(&gpu.dc, path, *frame_ms).ok();
                            if gpu.images.len() > 8 {
                                gpu.images.clear();
                            }
                            gpu.images.insert(key.clone(), b);
                        }
                        if let Some(Some(bmp)) = gpu.images.get(&key) {
                            // Cover: crop the source to the target's aspect ratio.
                            let s = bmp.GetSize();
                            let dst = to_d2d(*rect);
                            let (dw, dh) = (dst.right - dst.left, dst.bottom - dst.top);
                            let src = if dw > 0.0 && dh > 0.0 && s.width > 0.0 && s.height > 0.0 {
                                let k = (dw / s.width).max(dh / s.height);
                                let (cw, ch) = (dw / k, dh / k);
                                let (x, y) = ((s.width - cw) / 2.0, (s.height - ch) / 2.0);
                                windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                                    left: x,
                                    top: y,
                                    right: x + cw,
                                    bottom: y + ch,
                                }
                            } else {
                                windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                                    left: 0.0,
                                    top: 0.0,
                                    right: s.width,
                                    bottom: s.height,
                                }
                            };
                            gpu.dc.DrawBitmap(
                                bmp,
                                Some(&raw const dst),
                                opacity.clamp(0.0, 1.0),
                                windows::Win32::Graphics::Direct2D::D2D1_INTERPOLATION_MODE_LINEAR,
                                Some(&raw const src),
                                None,
                            );
                        }
                    }
                    DrawCmd::PushClip { rect } => {
                        gpu.dc
                            .PushAxisAlignedClip(&to_d2d(*rect), D2D1_ANTIALIAS_MODE_ALIASED);
                        clips += 1;
                    }
                    DrawCmd::PopClip => {
                        if clips > 0 {
                            gpu.dc.PopAxisAlignedClip();
                            clips -= 1;
                        }
                    }
                }
            }
            // Unbalanced clips would make EndDraw fail.
            for _ in 0..clips {
                gpu.dc.PopAxisAlignedClip();
            }
            gpu.dc.EndDraw(None, None).map_err(classify)?;
            let t1 = Instant::now();
            gpu.swapchain
                .Present(1, DXGI_PRESENT(0))
                .ok()
                .map_err(classify)?;
            Ok(FrameStats {
                draw: t1 - t0,
                present: t1.elapsed(),
            })
        }
    }

    /// Animates the surface opacity from 0 to 1 over `ms` milliseconds with
    /// an ease-out cubic curve. The animation runs in DWM; the application
    /// draws no extra frames. `ms == 0` shows the surface immediately.
    pub fn fade_in(&self, ms: u16) -> Result<(), RenderError> {
        self.fade_from(0.0, ms)
    }

    /// Animates the surface opacity from `from` to 1 on the compositor (no
    /// redraws); used when a new design is applied. `ms == 0` jumps to 1.
    pub fn fade_from(&self, from: f32, ms: u16) -> Result<(), RenderError> {
        let from = from.clamp(0.0, 1.0);
        let k = 1.0 - f64::from(from);
        let Some(gpu) = self.gpu.as_ref() else {
            return Ok(());
        };
        // SAFETY: DirectComposition calls on objects owned by `gpu`.
        unsafe {
            if ms == 0 {
                gpu.effect.SetOpacity2(1.0).map_err(classify)?;
            } else {
                let d = f64::from(ms) / 1000.0;
                let a = gpu.comp.CreateAnimation().map_err(classify)?;
                // from + k(1 - (1 - t/d)^3) = from + k(3t/d - 3t²/d² + t³/d³)
                a.AddCubic(
                    0.0,
                    from,
                    (k * 3.0 / d) as f32,
                    (-k * 3.0 / (d * d)) as f32,
                    (k / (d * d * d)) as f32,
                )
                .map_err(classify)?;
                a.End(d, 1.0).map_err(classify)?;
                gpu.effect.SetOpacity(&a).map_err(classify)?;
            }
            gpu.comp.Commit().map_err(classify)
        }
    }

    /// Returns temporary GPU memory to the driver while the surface is
    /// hidden (IDXGIDevice3::Trim).
    pub fn trim(&self) {
        if let Some(gpu) = &self.gpu
            && let Ok(d3) = gpu.dxgi.cast::<IDXGIDevice3>()
        {
            // SAFETY: documented call on our own device; the D3D11 context is
            // idle because frames are only drawn on demand.
            unsafe { d3.Trim() };
        }
        let _ = self.gpu.as_ref().map(|g| &g.d3d);
    }
}

fn to_d2d(r: windowsrice_ui::Rect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: r.x,
        top: r.y,
        right: r.right(),
        bottom: r.bottom(),
    }
}
