pub mod layout;
pub mod text_label;
pub mod image;
pub mod container;
pub mod background;
pub mod card;
pub mod button;

pub use image::{HAlign, VAlign};
pub use container::Container;
pub use layout::{Layout, UIElement};
pub use background::{Background, BackgroundTexture};
pub use card::Card;
pub use button::Button;

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
