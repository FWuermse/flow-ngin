use glyphon::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache};

pub(super) fn atlas() -> image::RgbaImage {
    let mut fonts =
        FontSystem::new_with_locale_and_db("en-US".into(), glyphon::fontdb::Database::new());
    let db = fonts.db_mut();
    db.load_font_data(include_bytes!("../../assets/fonts/Roboto-Regular.ttf").to_vec());
    db.set_sans_serif_family("Roboto");
    let mut text = Buffer::new(&mut fonts, Metrics::new(96.0, 160.0));
    text.set_size(Some(512.0), Some(640.0));
    text.set_text(
        "flow\nNGIN\nloading\nassets",
        &Attrs::new()
            .family(Family::Name("Roboto"))
            .letter_spacing(2.0 / 96.0),
        Shaping::Advanced,
        Some(glyphon::cosmic_text::Align::Center),
    );

    let mut atlas = image::RgbaImage::from_pixel(512, 640, image::Rgba([255, 255, 255, 0]));
    text.draw(
        &mut fonts,
        &mut SwashCache::new(),
        Color::rgb(255, 255, 255),
        |x, y, width, height, color| {
            for dy in -2..height as i32 + 2 {
                for dx in -2..width as i32 + 2 {
                    if let Some(pixel) =
                        atlas.get_pixel_mut_checked((x + dx) as u32, (y + dy) as u32)
                    {
                        pixel.0[3] = pixel.0[3].max(color.a());
                    }
                }
            }
        },
    );
    atlas
}
