use std::io::{BufReader, Cursor};

use crate::data_structures::{model, texture};

pub fn diffuse_normal_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
        label: Some("Model texture_bind_group_layout"),
    })
}

#[cfg(target_arch = "wasm32")]
fn format_url(file_name: &str) -> reqwest::Url {
    let window = web_sys::window().unwrap();
    let location = window.location();
    let mut origin = location.origin().unwrap();
    if !origin.ends_with("learn-wgpu") {
        origin = format!("{}/assets", origin);
    }
    let base = reqwest::Url::parse(&format!("{}/", origin,)).unwrap();
    base.join(file_name).unwrap()
}

pub async fn load_string(file_name: &str) -> anyhow::Result<String> {
    #[cfg(target_arch = "wasm32")]
    let txt = {
        let url = format_url(file_name);
        reqwest::get(url).await?.text().await?
    };
    #[cfg(not(target_arch = "wasm32"))]
    let txt = {
        // TODO: pass env for absolute path from lib caller
        let path = std::path::Path::new("./")
            .join("assets")
            .join(file_name);
        // TODO: use tokio if it's not wasm anyway. Most IO-load will be here
        std::fs::read_to_string(path)?
    };

    Ok(txt)
}

pub async fn load_binary(file_name: &str) -> anyhow::Result<Vec<u8>> {
    #[cfg(target_arch = "wasm32")]
    let data = {
        let url = format_url(file_name);
        reqwest::get(url).await?.bytes().await?.to_vec()
    };
    #[cfg(not(target_arch = "wasm32"))]
    // TODO make async
    let data = {
        // TODO: pass env for absolute path from lib caller
        let path = std::path::Path::new("./")
            .join("assets")
            .join(file_name);
        // TODO: use tokio if it's not wasm anyway. Most IO-load will be here
        std::fs::read(path)?
    };

    Ok(data)
}

pub async fn load_texture(
    file_name: &str,
    is_normal_map: bool,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: Option<&str>,
) -> anyhow::Result<texture::Texture> {
    let data = load_binary(file_name).await?;
    texture::Texture::from_bytes(device, queue, &data, file_name, format, is_normal_map)
}

pub async fn load_textures(
    file_name: &str,
    queue: &wgpu::Queue,
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
) -> anyhow::Result<(Vec<model::Material>, Vec<tobj::Model>)> {
    let obj_text: String = load_string(file_name).await?;
    // TODO: also make async if not wasm
    let obj_cursor = Cursor::new(obj_text);
    let mut obj_reader = BufReader::new(obj_cursor);

    let (models, obj_materials) = tobj::load_obj_buf_async(
        &mut obj_reader,
        &tobj::LoadOptions {
            triangulate: true,
            single_index: true,
            ..Default::default()
        },
        |p| async move {
            let mat_text = load_string(&p)
                .await
                .expect(format!("Material Texture not found for {p}.").as_str());
            tobj::load_mtl_buf(&mut BufReader::new(Cursor::new(mat_text)))
        },
    )
    .await?;

    // We rather use a default normal map when none is passed instead of changing the pipeline
    let mut materials = Vec::new();
    for m in obj_materials? {
        if let Some(m_diffuse_texture) = &m.diffuse_texture {
            let diffuse_texture =
                load_texture(&m_diffuse_texture, false, device, queue, None).await?;
            let normal_texture = match &m.normal_texture {
                Some(m_normal_texture) => {
                    load_texture(&m_normal_texture, true, device, queue, None).await?
                },
                None => texture::Texture::create_default_normal_map(1, 1, device, queue)
            };
            materials.push(model::Material::new(
                device,
                &m.name,
                diffuse_texture,
                normal_texture,
                layout,
            ));
        } else {
            // TODO: create cross-plattform abstraction
            log::error!("This material's mtl ({file_name}) references no texture.");
            println!("This material's mtl ({file_name}) references no texture.");
        }
    }
    Ok((materials, models))
}
