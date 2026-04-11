pub mod layout;
pub mod text_label;
pub mod image;
pub mod container;
pub mod background;
pub mod card;
pub mod button;
pub mod checkbox;
pub mod slider;
pub mod text_input;
pub mod grid;
pub mod vstack;
pub mod value;

pub use image::{HAlign, VAlign};
pub use container::Container;
pub use layout::{Layout, UIElement};
pub use background::{Background, BackgroundTexture};
pub use card::Card;
pub use button::Button;
pub use checkbox::Checkbox;
pub use slider::Slider;
pub use text_input::TextInput;
pub use grid::Grid;
pub use vstack::VStack;
pub use value::Value;

/// Alignment-based positioning within a parent's bounds.
///
/// `width`/`height` default to `None`, meaning the element fills the parent.
/// Use the builder methods to set explicit sizes or alignment.
#[derive(Clone, Copy)]
pub struct Placement {
    pub halign: HAlign,
    pub valign: VAlign,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            halign: HAlign::Left,
            valign: VAlign::Top,
            width: None,
            height: None,
        }
    }
}

impl Placement {
    /// Resolve absolute position and size from parent bounds.
    pub fn resolve(
        &self,
        parent_x: u32,
        parent_y: u32,
        parent_w: u32,
        parent_h: u32,
    ) -> (u32, u32, u32, u32) {
        let w = self.width.unwrap_or(parent_w);
        let h = self.height.unwrap_or(parent_h);
        let x = match self.halign {
            HAlign::Left => parent_x,
            HAlign::Center => parent_x + parent_w.saturating_sub(w) / 2,
            HAlign::Right => parent_x + parent_w.saturating_sub(w),
        };
        let y = match self.valign {
            VAlign::Top => parent_y,
            VAlign::Center => parent_y + parent_h.saturating_sub(h) / 2,
            VAlign::Bottom => parent_y + parent_h.saturating_sub(h),
        };
        (x, y, w, h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::image::{HAlign, VAlign};

    #[test]
    fn resolve_fills_parent_by_default() {
        let p = Placement::default();
        let (x, y, w, h) = p.resolve(10, 20, 800, 600);
        assert_eq!((x, y, w, h), (10, 20, 800, 600));
    }

    #[test]
    fn resolve_center_centers_element() {
        let p = Placement {
            halign: HAlign::Center,
            valign: VAlign::Center,
            width: Some(200),
            height: Some(100),
        };
        let (x, y, w, h) = p.resolve(0, 0, 800, 600);
        assert_eq!(w, 200);
        assert_eq!(h, 100);
        assert_eq!(x, (800 - 200) / 2);
        assert_eq!(y, (600 - 100) / 2);
    }

    #[test]
    fn resolve_right_bottom_aligns_to_corner() {
        let p = Placement {
            halign: HAlign::Right,
            valign: VAlign::Bottom,
            width: Some(100),
            height: Some(50),
        };
        let (x, y, _, _) = p.resolve(0, 0, 800, 600);
        assert_eq!(x, 700);
        assert_eq!(y, 550);
    }

    #[test]
    fn resolve_child_larger_than_parent_saturates_to_zero_offset() {
        // Child is wider/taller than parent
        let p = Placement {
            halign: HAlign::Center,
            valign: VAlign::Center,
            width: Some(1000),
            height: Some(800),
        };
        let (x, y, w, h) = p.resolve(10, 20, 800, 600);
        // saturating_sub(1000) from 800 = 0, so offset is 0
        assert_eq!(x, 10);
        assert_eq!(y, 20);
        assert_eq!(w, 1000);
        assert_eq!(h, 800);
    }

    #[test]
    fn resolve_with_nonzero_parent_origin() {
        let p = Placement {
            halign: HAlign::Center,
            valign: VAlign::Top,
            width: Some(100),
            height: None,
        };
        let (x, y, w, h) = p.resolve(50, 100, 400, 300);
        assert_eq!(x, 50 + (400 - 100) / 2);
        assert_eq!(y, 100);
        assert_eq!(w, 100);
        assert_eq!(h, 300);
    }

    #[test]
    fn resolve_odd_dimensions_truncate_correctly() {
        // (801 - 100) / 2 = 350 (integer division truncates)
        let p = Placement {
            halign: HAlign::Center,
            valign: VAlign::Center,
            width: Some(100),
            height: Some(100),
        };
        let (x, y, _, _) = p.resolve(0, 0, 801, 601);
        assert_eq!(x, 350); // (801-100)/2 = 350
        assert_eq!(y, 250); // (601-100)/2 = 250
    }
}
