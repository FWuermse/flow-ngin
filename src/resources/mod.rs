use crate::{data_structures::{model::{self}}, resources::texture::diffuse_normal_layout};

pub mod texture;
pub mod mesh;
pub mod pick;


pub async fn load_model_obj(
    file_name: &str,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> anyhow::Result<model::Model> {
    let bind_group_layout = diffuse_normal_layout(device);

    let (materials, models) = texture::load_textures(file_name, queue, device, &bind_group_layout).await?;
    let meshes = mesh::load_meshes(&models, file_name, device);

    let model = model::Model {
        meshes,
        materials,
    };
    Ok(model)
}