//! UI spike candidate A: Slint 1.19 (winit backend, FemtoVG OpenGL renderer).
//! EXPERIMENTAL prototype for measurement only; not distributed.
//!
//! Same content, tokens, geometry, DWM treatment and instrumentation as the
//! Direct2D candidate (see `spike-common`).

#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::winit_030::winit::platform::windows::WindowAttributesExtWindows;
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};
use spike_common::{
    Args, Backdrop, INPUT_INDICATOR, Interaction, Rgba, STATUS_ICONS, Stats, Tokens, WINDOW_TITLE,
    WORKSPACES, apply_window_effects, bar_geometry, clock_text, enable_per_monitor_dpi,
    until_next_minute,
};
use windows::Win32::Foundation::HWND;

slint::slint! {
    #[edition = "1.19"]
    component Pill inherits Rectangle {
        in property <string> label;
        in property <bool> is-selected;
        in property <brush> idle;
        in property <brush> accent;
        in property <brush> fg;
        in property <brush> fg-on-accent;
        in property <brush> ring;
        in property <length> radius;
        in property <length> text-size;
        in property <duration> anim;
        callback activated();

        border-radius: radius;
        background: is-selected ? accent : idle;
        animate background { duration: anim; easing: cubic-bezier(0.2, 0.0, 0.0, 1.0); }
        accessible-role: button;
        accessible-label: "Workspace " + label;
        accessible-checked: is-selected;
        accessible-action-default => { activated(); }

        fs := FocusScope {
            key-pressed(event) => {
                if (event.text == " " || event.text == "\n") {
                    root.activated();
                    return accept;
                }
                reject
            }
        }
        TouchArea {
            clicked => {
                fs.focus();
                root.activated();
            }
        }
        Text {
            text: label;
            font-size: text-size;
            font-weight: is-selected ? 500 : 400;
            color: is-selected ? fg-on-accent : fg;
            horizontal-alignment: center;
            vertical-alignment: center;
        }
        Rectangle {
            visible: fs.has-focus;
            x: -1px;
            y: -1px;
            width: parent.width + 2px;
            height: parent.height + 2px;
            border-width: 1.5px;
            border-color: ring;
            border-radius: radius + 1px;
        }
    }

    export component Bar inherits Window {
        in property <brush> surface;
        in property <length> panel-radius;
        in property <brush> pill-idle;
        in property <brush> accent;
        in property <brush> text-primary;
        in property <brush> text-secondary;
        in property <brush> text-on-accent;
        in property <brush> focus-ring;
        in property <string> font;
        in property <string> icon-font;
        in property <length> body;
        in property <length> pill-w;
        in property <length> pill-h;
        in property <length> pill-gap;
        in property <length> pad-x;
        in property <length> gap;
        in property <length> ctl-radius;
        in property <duration> anim;
        in property <string> window-title;
        in property <string> time;
        in property <string> date;
        in property <string> input-indicator;
        in property <[string]> icons;
        in property <[string]> workspaces;
        in-out property <int> selected: 0;
        callback select(int);

        title: "windowsRICE spike — Slint";
        no-frame: true;
        always-on-top: true;
        background: transparent;
        default-font-family: font;
        default-font-weight: 400;
        default-font-size: body;

        Rectangle {
            background: root.surface;
            border-radius: root.panel-radius;

            HorizontalLayout {
                padding-left: root.pad-x;
                padding-right: root.pad-x;
                spacing: 0px;

                HorizontalLayout {
                    spacing: root.pill-gap;
                    alignment: start;
                    for ws[i] in root.workspaces: VerticalLayout {
                        alignment: center;
                        Pill {
                            width: root.pill-w;
                            height: root.pill-h;
                            label: ws;
                            is-selected: root.selected == i;
                            idle: root.pill-idle;
                            accent: root.accent;
                            fg: root.text-primary;
                            fg-on-accent: root.text-on-accent;
                            ring: root.focus-ring;
                            radius: root.ctl-radius;
                            text-size: root.body;
                            anim: root.anim;
                            activated => { root.select(i); }
                        }
                    }
                }
                Rectangle { width: root.gap; }
                Text {
                    text: root.window-title;
                    color: root.text-secondary;
                    vertical-alignment: center;
                }
                Rectangle { horizontal-stretch: 1; }
                HorizontalLayout {
                    spacing: root.gap;
                    for icon in root.icons: Text {
                        text: icon;
                        font-family: root.icon-font;
                        font-size: root.body + 1px;
                        color: root.text-primary;
                        vertical-alignment: center;
                        accessible-role: text;
                    }
                    Text {
                        text: root.input-indicator;
                        color: root.text-primary;
                        vertical-alignment: center;
                    }
                }
            }

            center-group := HorizontalLayout {
                spacing: 8px;
                height: parent.height;
                width: self.preferred-width;
                x: (parent.width - self.preferred-width) / 2;
                Text {
                    text: root.time;
                    font-weight: 500;
                    color: root.text-primary;
                    vertical-alignment: center;
                }
                Text {
                    text: root.date;
                    color: root.text-secondary;
                    vertical-alignment: center;
                }
            }
        }
    }
}

fn color(c: Rgba) -> Color {
    Color::from_argb_encoded(c.argb_u32())
}

struct Anim {
    started: Instant,
    frames: u32,
    latency_recorded: bool,
}

fn main() -> Result<(), slint::PlatformError> {
    let args = Args::parse();
    enable_per_monitor_dpi();
    let tokens = Tokens::windowsrice_default_dark();
    let geo = bar_geometry(&tokens, args.scale_override);
    if args.scale_override.is_some() {
        // SAFETY: set before any other thread exists and before Slint starts.
        unsafe { std::env::set_var("SLINT_SCALE_FACTOR", geo.scale.to_string()) };
    }

    let renderer = args
        .renderer
        .clone()
        .unwrap_or_else(|| "femtovg".to_owned());
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name(renderer.clone())
        .with_winit_window_attributes_hook(|attrs| attrs.with_skip_taskbar(true).with_active(false))
        .select()?;

    let ui = Bar::new()?;
    let renderer_desc = match renderer.as_str() {
        "software" => "Slint 1.19.0, winit 0.30 backend, software renderer (CPU, softbuffer)",
        _ => "Slint 1.19.0, winit 0.30 backend, FemtoVG renderer (OpenGL via WGL)",
    };
    let stats = Rc::new(RefCell::new(Stats::new(
        "slint",
        renderer_desc,
        geo.scale,
        args.backdrop,
    )));
    let anim: Rc<RefCell<Option<Anim>>> = Rc::new(RefCell::new(None));

    // Tokens → properties.
    let surface = match args.backdrop {
        Backdrop::Acrylic => tokens.surface.with_alpha(tokens.surface_opacity),
        Backdrop::None => tokens.surface,
    };
    ui.set_surface(color(surface).into());
    ui.set_panel_radius(match args.backdrop {
        Backdrop::Acrylic => 0.0, // DWM rounds the window, as in the Direct2D candidate
        Backdrop::None => tokens.radius_panel,
    });
    ui.set_pill_idle(color(tokens.pill_idle).into());
    ui.set_accent(color(tokens.accent).into());
    ui.set_text_primary(color(tokens.text_primary).into());
    ui.set_text_secondary(color(tokens.text_secondary).into());
    ui.set_text_on_accent(color(tokens.text_on_accent).into());
    ui.set_focus_ring(color(tokens.focus_ring).into());
    ui.set_font(SharedString::from(tokens.font_family.as_str()));
    ui.set_icon_font(tokens.icon_family.into());
    ui.set_body(tokens.body_px);
    ui.set_pill_w(tokens.pill_width);
    ui.set_pill_h(tokens.pill_height);
    ui.set_pill_gap(tokens.pill_gap);
    ui.set_pad_x(tokens.padding_x);
    ui.set_gap(tokens.gap);
    ui.set_ctl_radius(tokens.radius_control);
    #[allow(clippy::cast_possible_truncation)]
    ui.set_anim(tokens.anim_ms as i64);
    ui.set_window_title(WINDOW_TITLE.into());
    ui.set_input_indicator(INPUT_INDICATOR.into());
    ui.set_icons(ModelRc::new(VecModel::from(
        STATUS_ICONS
            .iter()
            .map(|s| SharedString::from(*s))
            .collect::<Vec<_>>(),
    )));
    ui.set_workspaces(ModelRc::new(VecModel::from(
        WORKSPACES
            .iter()
            .map(|s| SharedString::from(*s))
            .collect::<Vec<_>>(),
    )));
    let (time, date) = clock_text();
    ui.set_time(time.into());
    ui.set_date(date.into());

    // Geometry (physical pixels), identical to the Direct2D candidate.
    ui.window()
        .set_position(slint::PhysicalPosition::new(geo.x, geo.y));
    ui.window()
        .set_size(slint::PhysicalSize::new(geo.width, geo.height));

    // Selection, with the same latency/animation instrumentation.
    let select = {
        let weak = ui.as_weak();
        let anim = anim.clone();
        move |i: i32| {
            if let Some(ui) = weak.upgrade()
                && ui.get_selected() != i
            {
                *anim.borrow_mut() = Some(Anim {
                    started: Instant::now(),
                    frames: 0,
                    latency_recorded: false,
                });
                ui.set_selected(i);
            }
        }
    };
    ui.on_select(select.clone());

    // Frame counting.
    {
        let stats = stats.clone();
        let anim = anim.clone();
        let anim_ms = f64::from(tokens.anim_ms);
        let stats_for_note = stats.clone();
        let notifier_error = ui
            .window()
            .set_rendering_notifier(move |state, _| {
                if let slint::RenderingState::AfterRendering = state {
                    let mut s = stats.borrow_mut();
                    s.record_frame();
                    let mut a = anim.borrow_mut();
                    if let Some(an) = a.as_mut() {
                        an.frames += 1;
                        let elapsed = an.started.elapsed().as_secs_f64() * 1000.0;
                        if !an.latency_recorded {
                            an.latency_recorded = true;
                            s.interactions.push(Interaction {
                                latency_ms: elapsed,
                                anim_frames: 0,
                                anim_ms: 0.0,
                            });
                        }
                        if elapsed >= anim_ms {
                            if let Some(last) = s.interactions.last_mut() {
                                last.anim_frames = an.frames;
                                last.anim_ms = elapsed;
                            }
                            *a = None;
                        }
                    }
                }
            })
            .err()
            .map(|e| {
                format!(
                    "rendering notifier unavailable with this renderer ({e:?}); frames not counted"
                )
            });
        if let Some(note) = notifier_error {
            stats_for_note.borrow_mut().window_log.push(note);
        }
    }

    // Window treatment once winit has created the native window. winit
    // creates it after the event loop starts, so poll briefly (up to 2 s).
    let effects_timer = Rc::new(slint::Timer::default());
    {
        let weak = ui.as_weak();
        let stats = stats.clone();
        let backdrop = args.backdrop;
        let started = Instant::now();
        let t = effects_timer.clone();
        effects_timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(10),
            move || {
                let Some(ui) = weak.upgrade() else { return };
                let handle = ui.window().window_handle();
                let hwnd = handle
                    .window_handle()
                    .ok()
                    .and_then(|wh| match wh.as_raw() {
                        RawWindowHandle::Win32(w) => {
                            Some(HWND(w.hwnd.get() as *mut core::ffi::c_void))
                        }
                        _ => None,
                    });
                if let Some(hwnd) = hwnd {
                    let mut log = apply_window_effects(hwnd, backdrop);
                    log.push(format!(
                        "applied after {} ms",
                        started.elapsed().as_millis()
                    ));
                    stats.borrow_mut().window_log = log;
                    t.stop();
                } else if started.elapsed() > Duration::from_secs(2) {
                    stats.borrow_mut().window_log =
                        vec!["native window handle unavailable after 2 s".into()];
                    t.stop();
                }
            },
        );
    }

    // Clock: redraw once per minute, exactly like the Direct2D candidate.
    let clock = Rc::new(slint::Timer::default());
    fn arm(clock: &Rc<slint::Timer>, weak: slint::Weak<Bar>) {
        let c = clock.clone();
        clock.start(
            slint::TimerMode::SingleShot,
            until_next_minute(),
            move || {
                if let Some(ui) = weak.upgrade() {
                    let (time, date) = clock_text();
                    if ui.get_time() != time.as_str() {
                        ui.set_time(time.into());
                    }
                    if ui.get_date() != date.as_str() {
                        ui.set_date(date.into());
                    }
                }
                arm(&c, weak.clone());
            },
        );
    }
    arm(&clock, ui.as_weak());

    let interaction_timer = slint::Timer::default();
    if args.interaction_test {
        let weak = ui.as_weak();
        let select = select.clone();
        interaction_timer.start(
            slint::TimerMode::Repeated,
            Duration::from_secs(2),
            move || {
                if let Some(ui) = weak.upgrade() {
                    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
                    let n = WORKSPACES.len() as i32;
                    select((ui.get_selected() + 1) % n);
                }
            },
        );
    }

    if let Some(d) = args.exit_after {
        slint::Timer::single_shot(d, || {
            let _ = slint::quit_event_loop();
        });
    }

    ui.run()?;
    if let Some(p) = &args.stats_path {
        stats.borrow().write(p);
    }
    Ok(())
}
