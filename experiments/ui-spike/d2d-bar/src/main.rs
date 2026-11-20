//! UI spike candidate B: Direct2D + DirectWrite + DirectComposition.
//! EXPERIMENTAL prototype for measurement only.
//!
//! Pipeline: D3D11 device (BGRA) → DXGI swap chain for composition
//! (premultiplied alpha, flip-sequential, 2 buffers) → Direct2D device
//! context draws into the back buffer → DirectComposition visual hosts the
//! swap chain on a WS_EX_NOREDIRECTIONBITMAP window. DWM draws the Acrylic
//! backdrop behind translucent pixels. Frames are produced only when content
//! changes; during the selection animation frames are paced by Present(1).

#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::time::Instant;

use spike_common::{
    Args, Backdrop, BarGeometry, INPUT_INDICATOR, Interaction, Rgba, STATUS_ICONS, Stats, Tokens,
    WINDOW_TITLE, WORKSPACES, apply_window_effects, bar_geometry, clock_text, ease,
    enable_per_monitor_dpi, until_next_minute,
};
use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1,
    D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_DRAW_TEXT_OPTIONS_NONE,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_ROUNDED_RECT, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,
    D2D1CreateFactory, ID2D1Bitmap1, ID2D1DeviceContext, ID2D1Factory1, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_AXIS_TAG_WEIGHT, DWRITE_FONT_AXIS_VALUE,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_METRICS, DWriteCreateFactory, IDWriteFactory,
    IDWriteTextFormat3, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_UNKNOWN,
    DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, DXGI_PRESENT, DXGI_SCALING_STRETCH,
    DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
    DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIDevice, IDXGIFactory2, IDXGISurface, IDXGISwapChain1,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, GetKeyState, VK_RETURN, VK_SHIFT, VK_SPACE, VK_TAB,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, IDC_ARROW, KillTimer, LoadCursorW, MSG, PM_REMOVE, PeekMessageW, PostQuitMessage,
    RegisterClassExW, SWP_NOACTIVATE, SWP_NOZORDER, SetTimer, SetWindowPos, TranslateMessage,
    WM_DESTROY, WM_DPICHANGED, WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_QUIT, WM_SETFOCUS,
    WM_TIMER, WNDCLASSEXW, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    WS_VISIBLE,
};
use windows::core::{HSTRING, Interface, Result, w};
use windows_numerics::Vector2;

const TIMER_CLOCK: usize = 1;
const TIMER_INTERACTION: usize = 2;
const TIMER_EXIT: usize = 3;

fn color(c: Rgba) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
}

struct Gpu {
    swapchain: IDXGISwapChain1,
    dc: ID2D1DeviceContext,
    target: Option<ID2D1Bitmap1>,
    brush: ID2D1SolidColorBrush,
    dwrite: IDWriteFactory,
    _comp_device: IDCompositionDevice,
    _comp_target: IDCompositionTarget,
    _visual: IDCompositionVisual,
}

struct Anim {
    from: usize,
    to: usize,
    started: Instant,
    changed_at: Instant,
    frames: u32,
    latency_recorded: bool,
}

struct App {
    hwnd: HWND,
    args: Args,
    tokens: Tokens,
    geo: BarGeometry,
    gpu: Gpu,
    stats: Stats,
    selected: usize,
    focused: usize,
    has_focus: bool,
    anim: Option<Anim>,
    time: String,
    date: String,
    dirty: bool,
    pill_rects: [D2D_RECT_F; 4],
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn create_gpu(hwnd: HWND, geo: &BarGeometry) -> Result<Gpu> {
    // SAFETY: standard D3D11/DXGI/D2D/DComp initialization on this thread;
    // all out-parameters are valid and every object is owned by `Gpu`.
    unsafe {
        let mut device: Option<ID3D11Device> = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&raw mut device),
            None,
            None,
        )?;
        let device = device.expect("D3D11 device");
        let dxgi_device: IDXGIDevice = device.cast()?;
        let factory: IDXGIFactory2 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))?;
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: geo.width,
            Height: geo.height,
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
        let swapchain = factory.CreateSwapChainForComposition(&device, &desc, None)?;

        let d2d: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let d2d_device = d2d.CreateDevice(&dxgi_device)?;
        let dc = d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
        // Grayscale AA: ClearType requires an opaque target.
        dc.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
        let brush = dc.CreateSolidColorBrush(
            &color(Rgba {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            }),
            None,
        )?;
        let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;

        let comp_device: IDCompositionDevice = DCompositionCreateDevice(&dxgi_device)?;
        let comp_target = comp_device.CreateTargetForHwnd(hwnd, true)?;
        let visual = comp_device.CreateVisual()?;
        visual.SetContent(&swapchain)?;
        comp_target.SetRoot(&visual)?;
        comp_device.Commit()?;

        let mut gpu = Gpu {
            swapchain,
            dc,
            target: None,
            brush,
            dwrite,
            _comp_device: comp_device,
            _comp_target: comp_target,
            _visual: visual,
        };
        bind_target(&mut gpu)?;
        Ok(gpu)
    }
}

fn bind_target(gpu: &mut Gpu) -> Result<()> {
    // SAFETY: the back buffer belongs to our swap chain.
    unsafe {
        let surface: IDXGISurface = gpu.swapchain.GetBuffer(0)?;
        let props = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
            ..Default::default()
        };
        let bmp = gpu.dc.CreateBitmapFromDxgiSurface(&surface, Some(&props))?;
        gpu.dc.SetTarget(&bmp);
        gpu.target = Some(bmp);
    }
    Ok(())
}

impl App {
    fn s(&self, v: f32) -> f32 {
        v * self.geo.scale
    }

    fn layout(
        &self,
        text: &str,
        family: &str,
        weight: u16,
        size: f32,
        max_w: f32,
    ) -> Result<IDWriteTextLayout> {
        // SAFETY: DirectWrite calls with owned strings.
        unsafe {
            let fmt = self.gpu.dwrite.CreateTextFormat(
                &HSTRING::from(family),
                None,
                DWRITE_FONT_WEIGHT(i32::from(weight)),
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                self.s(size),
                w!("ko-KR"),
            )?;
            fmt.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            // Segoe UI Variable has no named 500 instance; without an explicit
            // axis value DirectWrite snaps 500 to Semibold (600). Set the
            // variable-font weight axis to the exact token value.
            if let Ok(fmt3) = fmt.cast::<IDWriteTextFormat3>() {
                #[allow(clippy::cast_precision_loss)]
                let axis = [DWRITE_FONT_AXIS_VALUE {
                    axisTag: DWRITE_FONT_AXIS_TAG_WEIGHT,
                    value: f32::from(weight),
                }];
                fmt3.SetFontAxisValues(&axis)?;
            }
            let wide: Vec<u16> = text.encode_utf16().collect();
            #[allow(clippy::cast_precision_loss)]
            let h = self.geo.height as f32;
            self.gpu.dwrite.CreateTextLayout(&wide, &fmt, max_w, h)
        }
    }

    fn text_width(layout: &IDWriteTextLayout) -> f32 {
        let mut m = DWRITE_TEXT_METRICS::default();
        // SAFETY: valid layout and out-parameter.
        let _ = unsafe { layout.GetMetrics(&raw mut m) };
        m.widthIncludingTrailingWhitespace
    }

    fn fill(&self, rect: D2D_RECT_F, radius: f32, c: Rgba) {
        // SAFETY: drawing between BeginDraw/EndDraw on our device context.
        unsafe {
            self.gpu.brush.SetColor(&color(c));
            if radius > 0.0 {
                let rr = D2D1_ROUNDED_RECT {
                    rect,
                    radiusX: radius,
                    radiusY: radius,
                };
                self.gpu.dc.FillRoundedRectangle(&rr, &self.gpu.brush);
            } else {
                self.gpu.dc.FillRectangle(&rect, &self.gpu.brush);
            }
        }
    }

    fn text(&self, layout: &IDWriteTextLayout, x: f32, c: Rgba) {
        // SAFETY: drawing between BeginDraw/EndDraw.
        unsafe {
            self.gpu.brush.SetColor(&color(c));
            self.gpu.dc.DrawTextLayout(
                Vector2 { X: x, Y: 0.0 },
                layout,
                &self.gpu.brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
        }
    }

    fn pill_color(&self, i: usize) -> Rgba {
        let t = &self.tokens;
        if let Some(a) = &self.anim {
            #[allow(clippy::cast_precision_loss)]
            let p = ease((a.started.elapsed().as_secs_f32() * 1000.0 / t.anim_ms).min(1.0));
            if i == a.to {
                return t.pill_idle.lerp(t.accent, p);
            }
            if i == a.from {
                return t.accent.lerp(t.pill_idle, p);
            }
        }
        if i == self.selected {
            t.accent
        } else {
            t.pill_idle
        }
    }

    fn render(&mut self) -> Result<()> {
        let t = self.tokens.clone();
        #[allow(clippy::cast_precision_loss)]
        let (w, h) = (self.geo.width as f32, self.geo.height as f32);
        let body = t.body_px;

        // Layouts (rebuilt per frame: a handful of short strings).
        let title = self.layout(WINDOW_TITLE, &t.font_family, t.weight_regular, body, w)?;
        let time = self.layout(&self.time, &t.font_family, t.weight_emphasis, body, w)?;
        let date = self.layout(&self.date, &t.font_family, t.weight_regular, body, w)?;
        let input = self.layout(INPUT_INDICATOR, &t.font_family, t.weight_regular, body, w)?;
        let icons: Vec<IDWriteTextLayout> = STATUS_ICONS
            .iter()
            .map(|g| self.layout(g, t.icon_family, t.weight_regular, body + 1.0, w))
            .collect::<Result<_>>()?;
        let digits: Vec<IDWriteTextLayout> = WORKSPACES
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let weight = if i == self.selected {
                    t.weight_emphasis
                } else {
                    t.weight_regular
                };
                self.layout(d, &t.font_family, weight, body, self.s(t.pill_width))
            })
            .collect::<Result<_>>()?;

        // SAFETY: frame drawing on our device context and swap chain.
        unsafe {
            self.gpu.dc.BeginDraw();
            self.gpu.dc.Clear(Some(&color(Rgba {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            })));
            let full = D2D_RECT_F {
                left: 0.0,
                top: 0.0,
                right: w,
                bottom: h,
            };
            match self.args.backdrop {
                // DWM rounds the window corners and draws Acrylic behind us.
                Backdrop::Acrylic => self.fill(full, 0.0, t.surface.with_alpha(t.surface_opacity)),
                Backdrop::None => self.fill(full, self.s(t.radius_panel), t.surface),
            }

            // Left: workspace pills + window title.
            let mut x = self.s(t.padding_x);
            let ph = self.s(t.pill_height);
            let top = (h - ph) / 2.0;
            for i in 0..WORKSPACES.len() {
                let r = D2D_RECT_F {
                    left: x,
                    top,
                    right: x + self.s(t.pill_width),
                    bottom: top + ph,
                };
                self.pill_rects[i] = r;
                self.fill(r, self.s(t.radius_control), self.pill_color(i));
                let fg = if i == self.selected {
                    t.text_on_accent
                } else {
                    t.text_primary
                };
                let dw = Self::text_width(&digits[i]);
                self.text(&digits[i], x + (self.s(t.pill_width) - dw) / 2.0, fg);
                if self.has_focus && i == self.focused {
                    self.gpu.brush.SetColor(&color(t.focus_ring));
                    let inset = self.s(1.0);
                    let fr = D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F {
                            left: r.left - inset,
                            top: r.top - inset,
                            right: r.right + inset,
                            bottom: r.bottom + inset,
                        },
                        radiusX: self.s(t.radius_control + 1.0),
                        radiusY: self.s(t.radius_control + 1.0),
                    };
                    self.gpu
                        .dc
                        .DrawRoundedRectangle(&fr, &self.gpu.brush, self.s(1.5), None);
                }
                x += self.s(t.pill_width + t.pill_gap);
            }
            x += self.s(t.gap - t.pill_gap);
            self.text(&title, x, t.text_secondary);

            // Center: time + date.
            let (tw, dw) = (Self::text_width(&time), Self::text_width(&date));
            let gap = self.s(8.0);
            let cx = (w - (tw + gap + dw)) / 2.0;
            self.text(&time, cx, t.text_primary);
            self.text(&date, cx + tw + gap, t.text_secondary);

            // Right: icons + input indicator.
            let mut rx = w - self.s(t.padding_x);
            let iw = Self::text_width(&input);
            rx -= iw;
            self.text(&input, rx, t.text_primary);
            for icon in icons.iter().rev() {
                rx -= self.s(t.gap) + Self::text_width(icon);
                self.text(icon, rx, t.text_primary);
            }

            self.gpu.dc.EndDraw(None, None)?;
            self.gpu.swapchain.Present(1, DXGI_PRESENT(0)).ok()?;
        }
        self.stats.record_frame();
        self.dirty = false;

        if let Some(a) = &mut self.anim {
            a.frames += 1;
            if !a.latency_recorded {
                a.latency_recorded = true;
                self.stats.interactions.push(Interaction {
                    latency_ms: a.changed_at.elapsed().as_secs_f64() * 1000.0,
                    anim_frames: 0,
                    anim_ms: 0.0,
                });
            }
            let elapsed = a.started.elapsed();
            if elapsed.as_secs_f32() * 1000.0 >= self.tokens.anim_ms {
                if let Some(last) = self.stats.interactions.last_mut() {
                    last.anim_frames = a.frames;
                    last.anim_ms = elapsed.as_secs_f64() * 1000.0;
                }
                self.anim = None;
                self.dirty = true; // final frame at rest state
            }
        }
        Ok(())
    }

    fn select(&mut self, i: usize) {
        if i == self.selected {
            return;
        }
        let now = Instant::now();
        self.anim = Some(Anim {
            from: self.selected,
            to: i,
            started: now,
            changed_at: now,
            frames: 0,
            latency_recorded: false,
        });
        self.selected = i;
        self.dirty = true;
    }

    fn refresh_clock(&mut self) {
        let (time, date) = clock_text();
        if time != self.time || date != self.date {
            self.time = time;
            self.date = date;
            self.dirty = true;
        }
        // SAFETY: timer on our own window.
        unsafe {
            SetTimer(
                Some(self.hwnd),
                TIMER_CLOCK,
                u32::try_from(until_next_minute().as_millis()).unwrap_or(60_000),
                None,
            )
        };
    }

    fn resize(&mut self) -> Result<()> {
        // SAFETY: releasing the target before resizing our own swap chain.
        unsafe {
            self.gpu.dc.SetTarget(None);
            self.gpu.target = None;
            self.gpu.swapchain.ResizeBuffers(
                0,
                self.geo.width,
                self.geo.height,
                DXGI_FORMAT_UNKNOWN,
                DXGI_SWAP_CHAIN_FLAG(0),
            )?;
            SetWindowPos(
                self.hwnd,
                None,
                self.geo.x,
                self.geo.y,
                self.geo.width as i32,
                self.geo.height as i32,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )?;
        }
        bind_target(&mut self.gpu)?;
        self.dirty = true;
        Ok(())
    }
}

/// Runs `f` on the app state. Returns `None` if the state does not exist yet
/// or is already borrowed (a message re-entered the window procedure).
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => {
            if wparam.0 == TIMER_EXIT {
                // Outside any APP borrow: WM_DESTROY re-enters the window procedure.
                // SAFETY: destroying our own window.
                let _ = unsafe { DestroyWindow(hwnd) };
                return LRESULT(0);
            }
            with_app(|app| match wparam.0 {
                TIMER_CLOCK => app.refresh_clock(),
                TIMER_INTERACTION => {
                    let next = (app.selected + 1) % WORKSPACES.len();
                    app.select(next);
                }
                _ => {}
            });
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = (
                (lparam.0 & 0xFFFF) as i16 as f32,
                ((lparam.0 >> 16) & 0xFFFF) as i16 as f32,
            );
            with_app(|app| {
                if let Some(i) = app
                    .pill_rects
                    .iter()
                    .position(|r| x >= r.left && x <= r.right && y >= r.top && y <= r.bottom)
                {
                    app.focused = i;
                    app.select(i);
                }
            });
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let vk = u16::try_from(wparam.0).unwrap_or(0);
            with_app(|app| {
                let n = WORKSPACES.len();
                // SAFETY: GetKeyState has no preconditions.
                let shift = unsafe { GetKeyState(i32::from(VK_SHIFT.0)) } < 0;
                match vk {
                    v if v == VK_TAB.0 => {
                        app.focused = if shift {
                            (app.focused + n - 1) % n
                        } else {
                            (app.focused + 1) % n
                        }
                    }
                    v if v == VK_RETURN.0 || v == VK_SPACE.0 => {
                        let f = app.focused;
                        app.select(f);
                    }
                    _ => return,
                }
                app.dirty = true;
            });
            LRESULT(0)
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            let handled = with_app(|app| {
                app.has_focus = msg == WM_SETFOCUS;
                app.dirty = true;
                let what = if msg == WM_SETFOCUS {
                    "WM_SETFOCUS"
                } else {
                    "WM_KILLFOCUS"
                };
                let t = app.stats.started.elapsed().as_secs_f64();
                app.stats.window_log.push(format!("{what} at {t:.2}s"));
            });
            if handled.is_none() {
                // Arrived before the app state exists or while it is borrowed.
                // SAFETY: default handling for our own window.
                return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            // Untested in this spike (changing Windows scaling is out of scope).
            with_app(|app| {
                app.geo = bar_geometry(&app.tokens, app.args.scale_override);
                let _ = app.resize();
            });
            LRESULT(0)
        }
        WM_DESTROY => {
            with_app(|app| {
                if let Some(p) = &app.args.stats_path {
                    app.stats.write(p);
                }
            });
            // SAFETY: ends our message loop.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        // SAFETY: default handling for our own window.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    enable_per_monitor_dpi();
    let tokens = Tokens::windowsrice_default_dark();
    let geo = bar_geometry(&tokens, args.scale_override);

    // SAFETY: window creation and message loop on the main thread.
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = w!("windowsRICE.spike.d2d");
        let wc = WNDCLASSEXW {
            cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).unwrap_or(0),
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassExW(&raw const wc);
        let hwnd = CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class,
            w!("windowsRICE spike — Direct2D"),
            WS_POPUP | WS_VISIBLE,
            geo.x,
            geo.y,
            geo.width as i32,
            geo.height as i32,
            None,
            None,
            Some(instance.into()),
            None,
        )?;

        let mut stats = Stats::new(
            "d2d",
            "Direct2D 1.1 + DirectWrite + DirectComposition (D3D11 hardware)",
            geo.scale,
            args.backdrop,
        );
        stats.window_log = apply_window_effects(hwnd, args.backdrop);
        let gpu = create_gpu(hwnd, &geo)?;
        let (time, date) = clock_text();

        if let Some(d) = args.exit_after {
            SetTimer(
                Some(hwnd),
                TIMER_EXIT,
                u32::try_from(d.as_millis()).unwrap_or(u32::MAX),
                None,
            );
        }
        if args.interaction_test {
            SetTimer(Some(hwnd), TIMER_INTERACTION, 2000, None);
        }
        let _ = RECT::default();
        APP.with(|a| {
            *a.borrow_mut() = Some(App {
                hwnd,
                args,
                tokens,
                geo,
                gpu,
                stats,
                selected: 0,
                focused: 0,
                has_focus: false,
                anim: None,
                time,
                date,
                dirty: true,
                pill_rects: [D2D_RECT_F::default(); 4],
            });
        });
        with_app(App::refresh_clock);
        // WM_SETFOCUS may have arrived during CreateWindowExW, before the app
        // state existed; read the current focus once state is initialized.
        let focused_now = GetFocus() == hwnd;
        with_app(|a| a.has_focus = focused_now);

        let mut msg = MSG::default();
        loop {
            let animating = with_app(|a| a.anim.is_some()).unwrap_or(false);
            if animating {
                // Drain input without blocking, then render a vsync-paced frame.
                while PeekMessageW(&raw mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    if msg.message == WM_QUIT {
                        return Ok(());
                    }
                    let _ = TranslateMessage(&raw const msg);
                    DispatchMessageW(&raw const msg);
                }
            } else {
                if !GetMessageW(&raw mut msg, None, 0, 0).as_bool() {
                    break;
                }
                let _ = TranslateMessage(&raw const msg);
                DispatchMessageW(&raw const msg);
            }
            let render = with_app(|a| a.dirty || a.anim.is_some()).unwrap_or(false);
            if render && let Some(Err(e)) = with_app(App::render) {
                eprintln!("render failed: {e}");
                break;
            }
        }
        let _ = KillTimer(Some(hwnd), TIMER_CLOCK);
    }
    // Release GPU objects deterministically, before thread-local teardown.
    APP.with(|a| drop(a.borrow_mut().take()));
    Ok(())
}
