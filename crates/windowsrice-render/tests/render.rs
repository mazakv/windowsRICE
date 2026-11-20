//! Real Windows tests: DirectWrite font resolution on this machine and a
//! full render / device-loss / recovery cycle on a hidden window. They need
//! a Windows 11 desktop session with the standard fonts.

use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassExW, WNDCLASSEXW,
    WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::w;
use windowsrice_render::{FontConfig, RenderError, Renderer, TextSystem};
use windowsrice_theme::Color;
use windowsrice_ui::Rect;
use windowsrice_ui::draw::{DrawCmd, DrawList, FontRole, TextStyle};
use windowsrice_ui::text::TextMeasure;

fn fonts() -> FontConfig {
    FontConfig {
        family: "Segoe UI Variable".into(),
        fallback: vec!["Malgun Gothic".into()],
        icon_family: "Segoe Fluent Icons".into(),
        icon_fallback: "Segoe MDL2 Assets".into(),
        locale: "ko-KR".into(),
    }
}

fn ui(weight: u16) -> TextStyle {
    TextStyle {
        font: FontRole::Ui,
        size: 13.0,
        weight,
    }
}

#[test]
fn hangul_falls_back_to_malgun_gothic() {
    let t = TextSystem::new(fonts()).unwrap();
    assert!(t.missing_families.is_empty(), "{:?}", t.missing_families);
    assert_eq!(t.icon_family(), "Segoe Fluent Icons");
    for s in ["문서", "한", "수요일", "ㄱ"] {
        assert_eq!(
            t.resolved_family(s, 400).unwrap().as_deref(),
            Some("Malgun Gothic"),
            "{s}"
        );
    }
    assert_eq!(
        t.resolved_family("Explorer", 400).unwrap().as_deref(),
        Some("Segoe UI Variable")
    );
}

#[test]
fn weight_500_is_exact_not_snapped() {
    let t = TextSystem::new(fonts()).unwrap();
    let text = "windowsRICE Settings 15:42";
    let w: Vec<f32> = [400, 500, 600]
        .iter()
        .map(|&wt| t.measure(text, &ui(wt)).w)
        .collect();
    assert!(
        w[0] < w[1] && w[1] < w[2],
        "widths {w:?}: 500 must differ from 400 and 600"
    );
}

#[test]
fn korean_and_mixed_text_measure() {
    let t = TextSystem::new(fonts()).unwrap();
    let k = t.measure("문서 — 파일 탐색기", &ui(400));
    let e = t.measure("Documents", &ui(400));
    assert!(k.w > 0.0 && e.w > 0.0);
    assert!(k.h > 13.0 && k.h < 30.0, "{k:?}");
}

unsafe extern "system" fn wndproc(
    h: windows::Win32::Foundation::HWND,
    m: u32,
    w: windows::Win32::Foundation::WPARAM,
    l: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    // SAFETY: default processing for our own test window.
    unsafe { DefWindowProcW(h, m, w, l) }
}

#[test]
fn render_device_loss_and_recovery_on_hidden_window() {
    // SAFETY: a hidden test window owned by this test; destroyed at the end.
    let hwnd = unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let wc = WNDCLASSEXW {
            cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).unwrap(),
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: w!("windowsRICE.render.test"),
            ..Default::default()
        };
        RegisterClassExW(&raw const wc);
        CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW,
            w!("windowsRICE.render.test"),
            w!("render test"),
            WS_POPUP,
            0,
            0,
            300,
            51,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap()
    };
    let text = TextSystem::new(fonts()).unwrap();
    let mut r = Renderer::new(hwnd, 300, 51, 144, text).unwrap();
    let info = r.info();
    assert!(!info.adapter.is_empty());
    assert!(info.feature_level >= 0x9100);

    let mut list = DrawList::default();
    list.push(DrawCmd::Fill {
        rect: Rect::new(0.0, 0.0, 200.0, 34.0),
        radius: 0.0,
        color: Color {
            a: 224,
            ..Color::rgb(0x20, 0x22, 0x28)
        },
        owner: None,
    });
    list.push(DrawCmd::Text {
        rect: Rect::new(8.0, 4.0, 150.0, 26.0),
        text: "문서 — 파일 탐색기".into(),
        style: ui(400),
        color: Color::WHITE,
        trim: true,
        owner: None,
    });
    list.push(DrawCmd::Stroke {
        rect: Rect::new(6.0, 3.0, 120.0, 28.0),
        radius: 7.0,
        width: 2.0,
        color: Color::WHITE,
        owner: None,
    });
    r.render(&list).unwrap();

    r.simulate_device_loss();
    assert!(matches!(r.render(&list), Err(RenderError::DeviceLost(_))));
    r.recover().unwrap();
    r.render(&list).unwrap();
    assert_eq!(r.info().recoveries, 1);

    r.resize(400, 68, 192).unwrap();
    r.render(&list).unwrap();
    r.fade_in(120).unwrap();
    r.trim();
    drop(r);
    // SAFETY: destroying our own window.
    unsafe { DestroyWindow(hwnd).unwrap() };
}

#[test]
fn installed_font_families_are_enumerated_with_hangul_support() {
    let t = TextSystem::new(fonts()).unwrap();
    let t0 = std::time::Instant::now();
    let fams = t.installed_families();
    let took = t0.elapsed();
    let text: Vec<_> = fams.iter().filter(|f| !f.symbol).collect();
    eprintln!(
        "{} families ({} for text) in {took:?}",
        fams.len(),
        text.len()
    );
    let malgun = fams
        .iter()
        .find(|f| f.name == "Malgun Gothic")
        .expect("Malgun Gothic");
    assert!(malgun.hangul && malgun.weights.contains(&400), "{malgun:?}");
    let segoe = fams
        .iter()
        .find(|f| f.name == "Segoe UI Variable")
        .expect("Segoe UI Variable");
    assert!(!segoe.hangul, "Segoe UI Variable has no Hangul");
    assert!(segoe.weights.len() >= 3, "{segoe:?}");
    assert!(fams.iter().any(|f| f.symbol), "symbol fonts are flagged");
    // Unique names, sorted case-insensitively.
    let names: Vec<String> = fams.iter().map(|f| f.name.to_lowercase()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(names, sorted);
    assert!(took.as_millis() < 2000, "enumeration took {took:?}");
}
