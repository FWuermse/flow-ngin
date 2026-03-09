use std::sync::Arc;

use crate::{
    pipelines::gui::{mk_bind_group, mk_bind_group_layout},
    resources::texture::load_texture,
};

/// A GPU-resident background texture ready for binding.
///
/// Structurally mirrors `Atlas` but without grid layout — it covers the full image.
/// Wrap in `Arc` to share across multiple containers.
pub struct BackgroundTexture {
    pub(crate) bind_group: wgpu::BindGroup,
}

impl BackgroundTexture {
    /// Load a single image file as a background texture.
    pub async fn new(device: &wgpu::Device, queue: &wgpu::Queue, file_name: &str) -> Self {
        let texture = load_texture(file_name, false, device, queue, None)
            .await
            .unwrap();
        let texture_bind_group_layout = mk_bind_group_layout(device);
        let bind_group = mk_bind_group(device, &texture, &texture_bind_group_layout);
        BackgroundTexture { bind_group }
    }
}

/// Background variant for a `Container`.
pub enum Background {
    /// A solid RGBA colour rendered as a 1×1 texture stretched over the container.
    Color([u8; 4]),
    /// A pre-loaded texture covering the full container rect.
    Texture(Arc<BackgroundTexture>),
}
