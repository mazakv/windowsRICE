//! Window placement of the panel on a monitor.

use windowsrice_theme::schema::BarPosition;

use crate::geom::PhysRect;

/// A monitor as seen by the placement logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorGeometry {
    /// Full monitor rectangle in physical screen pixels.
    pub monitor: PhysRect,
    /// Work area (monitor minus the Windows taskbar and app bars).
    pub work: PhysRect,
    /// Effective DPI (96 = 100%).
    pub dpi: u32,
}

impl MonitorGeometry {
    pub fn scale(&self) -> f32 {
        self.dpi as f32 / 96.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelPlacement {
    pub height: f32,
    pub margin: f32,
    pub floating: bool,
    pub position: BarPosition,
}

/// Physical window rectangle for the panel. The panel is placed inside the
/// work area, so it never covers the Windows taskbar. Sizes are rounded to
/// whole pixels; the width is never negative.
pub fn panel_rect(m: &MonitorGeometry, scale: f32, p: &PanelPlacement) -> PhysRect {
    let px = |v: f32| (v * scale).round() as i32;
    let margin = if p.floating { px(p.margin) } else { 0 };
    let h = px(p.height).max(1);
    let left = m.work.left + margin;
    let right = (m.work.right - margin).max(left + 1);
    let top = match p.position {
        BarPosition::Top => m.work.top + margin,
        BarPosition::Bottom => m.work.bottom - margin - h,
    };
    PhysRect {
        left,
        top,
        right,
        bottom: top + h,
    }
}

/// Full-width bar rectangle: the whole width of the monitor (not the work
/// area), touching its top edge, `height` DIPs tall. An application desktop
/// toolbar registration then removes this strip from the work area.
pub fn bar_rect(monitor: PhysRect, scale: f32, height: f32) -> PhysRect {
    let h = ((height * scale).round() as i32).max(1);
    PhysRect {
        left: monitor.left,
        top: monitor.top,
        right: monitor.right,
        bottom: monitor.top + h,
    }
}

/// Collapsed auto-hide strip: the top `px` physical pixels of the bar area,
/// kept so that pointing at the top edge brings the bar back.
pub fn collapsed_rect(full: PhysRect, px: i32) -> PhysRect {
    PhysRect {
        bottom: full.top + px.max(1),
        ..full
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn mon(dpi: u32) -> MonitorGeometry {
        MonitorGeometry {
            monitor: PhysRect {
                left: 0,
                top: 0,
                right: 3840,
                bottom: 2160,
            },
            work: PhysRect {
                left: 0,
                top: 0,
                right: 3840,
                bottom: 2088,
            },
            dpi,
        }
    }

    const P: PanelPlacement = PanelPlacement {
        height: 34.0,
        margin: 8.0,
        floating: true,
        position: BarPosition::Top,
    };

    #[test]
    fn floating_top_at_each_scale() {
        for (dpi, h, m) in [(96, 34, 8), (120, 43, 10), (144, 51, 12), (192, 68, 16)] {
            let r = panel_rect(&mon(dpi), dpi as f32 / 96.0, &P);
            assert_eq!(r.height(), h, "dpi {dpi}");
            assert_eq!((r.left, r.top), (m, m), "dpi {dpi}");
            assert_eq!(r.right, 3840 - m, "dpi {dpi}");
        }
    }

    #[test]
    fn bottom_stays_above_taskbar() {
        let p = PanelPlacement {
            position: BarPosition::Bottom,
            ..P
        };
        let r = panel_rect(&mon(144), 1.5, &p);
        assert_eq!(r.bottom, 2088 - 12);
    }

    #[test]
    fn attached_has_no_margin() {
        let p = PanelPlacement {
            floating: false,
            ..P
        };
        let r = panel_rect(&mon(144), 1.5, &p);
        assert_eq!((r.left, r.top, r.right), (0, 0, 3840));
    }

    #[test]
    fn respects_offset_monitor_and_work_area() {
        let m = MonitorGeometry {
            monitor: PhysRect {
                left: -1920,
                top: 100,
                right: 0,
                bottom: 1180,
            },
            work: PhysRect {
                left: -1920,
                top: 148,
                right: 0,
                bottom: 1180,
            },
            dpi: 96,
        };
        let r = panel_rect(&m, 1.0, &P);
        assert_eq!((r.left, r.top, r.right, r.bottom), (-1912, 156, -8, 190));
    }

    #[test]
    fn bar_spans_the_monitor_edge_to_edge() {
        for (scale, h) in [(1.0, 34), (1.25, 43), (1.5, 51), (2.0, 68)] {
            let m = PhysRect {
                left: -2560,
                top: -200,
                right: 0,
                bottom: 1240,
            };
            let r = bar_rect(m, scale, 34.0);
            assert_eq!(
                (r.left, r.top, r.right),
                (m.left, m.top, m.right),
                "scale {scale}"
            );
            assert_eq!(r.height(), h);
            assert_eq!(collapsed_rect(r, 2).height(), 2);
            assert_eq!(collapsed_rect(r, 2).width(), r.width());
        }
    }
}
