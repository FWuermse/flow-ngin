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

/// Start position (x, y) and width/height in pixels
#[derive(Clone, Copy)]
pub struct Positioning {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}
