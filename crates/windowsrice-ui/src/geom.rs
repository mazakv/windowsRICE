//! Geometry in device-independent pixels (DIPs, 1/96 inch) and physical pixels.

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub w: f32,
    pub h: f32,
}

/// Axis-aligned rectangle in DIPs.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn center_y(&self) -> f32 {
        self.y + self.h / 2.0
    }

    /// Half-open containment: the right and bottom edges are outside.
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }

    /// True if `other` lies within `self`, allowing `eps` DIPs of overhang.
    pub fn contains_rect(&self, other: &Rect, eps: f32) -> bool {
        other.x >= self.x - eps
            && other.y >= self.y - eps
            && other.right() <= self.right() + eps
            && other.bottom() <= self.bottom() + eps
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    /// The overlapping part, if any.
    pub fn intersect(&self, other: &Rect) -> Option<Rect> {
        if !self.intersects(other) {
            return None;
        }
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        Some(Rect::new(
            x,
            y,
            self.right().min(other.right()) - x,
            self.bottom().min(other.bottom()) - y,
        ))
    }

    pub fn inflate(&self, d: f32) -> Rect {
        Rect::new(self.x - d, self.y - d, self.w + 2.0 * d, self.h + 2.0 * d)
    }

    pub fn union(&self, other: &Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Rect::new(
            x,
            y,
            self.right().max(other.right()) - x,
            self.bottom().max(other.bottom()) - y,
        )
    }

    /// Moves every edge to the nearest physical pixel boundary at `scale`, so
    /// fills and focus rings stay crisp at fractional scaling (125%, 150%).
    pub fn snap(&self, scale: f32) -> Rect {
        let s = |v: f32| (v * scale).round() / scale;
        let (l, t, r, b) = (s(self.x), s(self.y), s(self.right()), s(self.bottom()));
        Rect::new(l, t, r - l, b - t)
    }

    /// Converts to physical pixels relative to `origin` (the window's
    /// top-left corner in screen coordinates).
    pub fn to_physical(&self, scale: f32, origin: (i32, i32)) -> PhysRect {
        let px = |v: f32| (v * scale).round() as i32;
        PhysRect {
            left: origin.0 + px(self.x),
            top: origin.1 + px(self.y),
            right: origin.0 + px(self.right()),
            bottom: origin.1 + px(self.bottom()),
        }
    }
}

/// Rectangle in physical pixels (screen or window coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PhysRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl PhysRect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }

    pub fn is_empty(&self) -> bool {
        self.width() <= 0 || self.height() <= 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_aligns_edges_to_physical_pixels() {
        let r = Rect::new(10.3, 4.1, 25.2, 26.0).snap(1.5);
        for edge in [r.x, r.y, r.right(), r.bottom()] {
            let px = edge * 1.5;
            assert!((px - px.round()).abs() < 1e-3, "edge {edge} not on grid");
        }
    }

    #[test]
    fn containment_is_half_open() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(r.contains(Point { x: 0.0, y: 9.99 }));
        assert!(!r.contains(Point { x: 10.0, y: 5.0 }));
    }

    #[test]
    fn physical_conversion_rounds() {
        let p = Rect::new(1.0, 2.0, 3.0, 4.0).to_physical(1.25, (100, 200));
        assert_eq!(
            p,
            PhysRect {
                left: 101,
                top: 203,
                right: 105,
                bottom: 208
            }
        );
    }
}
