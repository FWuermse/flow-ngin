//! GPU textures and texture creation utilities.
//!
//! This module provides [`Texture`], a wrapper around WGPU GPU texture resources,
//! and helper methods for creating depth textures, normal maps, and loading textures
//! from image data.

use anyhow::*;
use image::{GenericImageView, ImageFormat, load_from_memory_with_format};

/// A GPU texture with a view and optional sampler.
///
/// Wraps WGPU texture objects along with associated views and samplers.
/// Textures are used for color maps, normal maps, depth, and other data
/// bound to shaders. Typically created via [`from_bytes`](Self::from_bytes) or
/// via [`create_depth_texture`](Self::create_depth_texture).
#[derive(Clone, Debug)]
pub struct Texture {
    #[allow(unused)]
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: Option<wgpu::Sampler>,
}

impl Texture {
    /// Standard depth buffer texture format (32-bit float).
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    /// Create a depth texture for depth-testing during rendering.
    ///
    /// Depth textures are required for proper depth-testing to determine which objects
    /// are in front of others. The returned texture is suitable for use as a
    /// `RENDER_ATTACHMENT` in render passes.
    ///
    /// # Arguments
    ///
    /// * `size` is [width, height] of the texture in pixels
    /// * `label` is used as a debug label for the GPU resource
    pub fn create_depth_texture(device: &wgpu::Device, size: [u32; 2], label: &str) -> Self {
        let size = wgpu::Extent3d {
            width: size[0].max(1),
            height: size[1].max(1),
            depth_or_array_layers: 1,
        };
        let desc = wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[Self::DEPTH_FORMAT],
        };
        let texture = device.create_texture(&desc);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = Some(device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            lod_min_clamp: 0.0,
            lod_max_clamp: 100.0,
            ..Default::default()
        }));

        Self {
            texture,
            view,
            sampler,
        }
    }

    /// Create a default normal map (neutral blue, representing no deformation).
    ///
    /// Returns a solid blue texture suitable as a default when no normal map is provided.
    /// This avoids the need to change shaders when normal maps are optional.
    pub fn create_default_normal_map(
        width: u32,
        height: u32,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Texture {
        let size = wgpu::Extent3d {
            width: width,
            height: height,
            depth_or_array_layers: 1,
        };

        // The blue/purple-ish colour that represents the default for normal maps
        let data: Vec<u8> = [127, 127, 255, 255]
            .iter()
            .cycle()
            .take(width as usize * height as usize * 4)
            .map(|&u| u)
            .collect();

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("default normal map"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                aspect: wgpu::TextureAspect::All,
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            &data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = Some(create_default_sampler(device));
        Texture {
            texture,
            view,
            sampler,
        }
    }

    /// Load a texture from raw byte data (image file contents).
    ///
    /// # Arguments
    ///
    /// * `bytes` represent raw image file data (PNG, JPEG, etc.)
    /// * `label` is used as a debug name for the GPU resource
    /// * `format`  is an optional file format hint (e.g., "png"). If None, auto-detect.
    /// * `is_normal_map` toggles between sRGB (false) and linear (true) color space
    pub fn from_bytes(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bytes: &[u8],
        label: &str,
        format: Option<&str>,
        is_normal_map: bool,
    ) -> Result<Self> {
        let img = match format {
            None => image::load_from_memory(bytes)?,
            Some(fmt) => {
                load_from_memory_with_format(bytes, ImageFormat::from_extension(fmt).unwrap())?
            }
        };
        Self::from_image(device, queue, &img, Some(label), is_normal_map)
    }

    pub fn from_image(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        img: &image::DynamicImage,
        label: Option<&str>,
        is_normal_map: bool,
    ) -> Result<Self> {
        let dimensions = img.dimensions();
        let rgba = img.to_rgba8();

        let size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth_or_array_layers: 1,
        };
        let format = if is_normal_map {
            wgpu::TextureFormat::Rgba8Unorm
        } else {
            wgpu::TextureFormat::Rgba8UnormSrgb
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                aspect: wgpu::TextureAspect::All,
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * dimensions.0),
                rows_per_image: Some(dimensions.1),
            },
            size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = Some(device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        }));

        Ok(Self {
            texture,
            view,
            sampler,
        })
    }
}

pub fn create_default_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    })
}
