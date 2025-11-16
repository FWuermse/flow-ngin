use std::{collections::HashMap, convert::identity, io::{BufReader, Cursor}};

use crate::{data_structures::{model::{self}, scene_graph::{to_scene_node, AnimationClip, ContainerNode, SceneNode}, texture::Texture}, resources::{animation::Keyframes, texture::{diffuse_normal_layout, load_binary, load_texture}}};

/**
 * This module contains all logic for loading mesh/textures/etc. from external files.
 */
pub mod animation;
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
    let meshes = meshes.into_iter().enumerate().filter_map(|(idx, result)| {
        match result {
            Ok(mesh) => Some(mesh),
            Err(_) => {
                log::warn!("Mesh at index {} in file {} could not be loaded due to overflows. Make sure you use the right scale in your .obj export settings.", idx, file_name);
                None
            },
        }
    }).collect();

    let model = model::Model {
        meshes,
        materials,
    };
    Ok(model)
}

pub async fn load_model_gltf(
    file_name: &str,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> anyhow::Result<Box<dyn SceneNode>> {
    let gltf_text = load_binary(file_name).await?;
    let gltf_cursor = Cursor::new(gltf_text);
    let gltf_reader = BufReader::new(gltf_cursor);
    let gltf = gltf::Gltf::from_reader(gltf_reader)?;

    // Load buffers
    let mut buffer_data = Vec::new();
    for buffer in gltf.buffers() {
        match buffer.source() {
            gltf::buffer::Source::Bin => {
                if let Some(blob) = gltf.blob.as_deref() {
                    buffer_data.push(blob.into());
                };
            }
            gltf::buffer::Source::Uri(uri) => {
                let bin = load_binary(uri).await?;
                buffer_data.push(bin);
            }
        }
    }
    // Load animations
    let mut animations: HashMap<usize, Vec<AnimationClip>> = HashMap::new();
    for animation in gltf.animations() {
        for channel in animation.channels() {
            let reader = channel.reader(|buffer| Some(&buffer_data[buffer.index()]));
            let timestamps = if let Some(inputs) = reader.read_inputs() {
                match inputs {
                    gltf::accessor::Iter::Standard(times) => {
                        let times: Vec<f32> = times.collect();
                        times
                    }
                    gltf::accessor::Iter::Sparse(_) => {
                        let times: Vec<f32> = Vec::new();
                        times
                    }
                }
            } else {
                println!("No animation found in channel {}", channel.index());
                let times: Vec<f32> = Vec::new();
                times
            };
            let keyframes = if let Some(outputs) = reader.read_outputs() {
                match outputs {
                    gltf::animation::util::ReadOutputs::Translations(translation) => {
                        let translation_vec = translation
                            .map(|tr| {
                                let vector = tr.into();
                                vector
                            })
                            .collect();
                        Keyframes::Translation(translation_vec)
                    }
                    gltf::animation::util::ReadOutputs::Rotations(rotation) => {
                        let quaternions: Vec<cgmath::Quaternion<f32>> = rotation.into_f32()
                            .map(|quat| {
                                let quat = quat.into();
                                quat
                            })
                            .collect(); 
                        Keyframes::Rotation(quaternions)
                    }
                    gltf::animation::util::ReadOutputs::Scales(scales) => {
                        let quaternion = scales
                            .map(|sc| {
                                let sc = sc.into();
                                sc
                            })
                            .collect(); 
                        Keyframes::Scale(quaternion)
                    }
                    // TODO: implement morphing
                    gltf::animation::util::ReadOutputs::MorphTargetWeights(_) => Keyframes::Other,
                }
            } else {
                println!("No Keyframes found in channel {}", channel.index());
                Keyframes::Other
            };
            let name = animation.name().unwrap_or("Default").to_string();
            let animation = AnimationClip {
                name,
                keyframes,
                timestamps,
            };
            animations.entry(channel.target().node().index()).and_modify(|v | v.push(animation.clone())).or_insert(vec![animation]);
        }
    }
    // Load materials
    let mut materials = Vec::new();
    for material in gltf.materials() {
        let pbr = material.pbr_metallic_roughness();
        let texture_source = &pbr
            .base_color_texture()
            .map(|tex| {
                tex.texture().source().source()
            })
            .expect("texture");
        let diffuse_texture = match texture_source {
            gltf::image::Source::View { view, mime_type } => {
                let diffuse_texture = Texture::from_bytes(
                    device,
                    queue,
                    &buffer_data[view.buffer().index()],
                    file_name,
                    mime_type.split('/').last(),
                    false,
                )
                .expect("Couldn't load diffuse");
                diffuse_texture
            }
            gltf::image::Source::Uri { uri, mime_type } => {
                let diffuse_texture = load_texture(
                    uri,
                    false,
                    device,
                    queue,
                    mime_type.map(|mt| mt.split('/').last().map_or("jpg", identity)),
                )
                .await?;
                diffuse_texture
            }
        };
        let normal_texture = if let Some(texture) = material.normal_texture() {
            match &texture.texture().source().source() {
                gltf::image::Source::View { view, mime_type: _ } => {
                    let texture = Texture::from_bytes(
                        device,
                        queue,
                        &buffer_data[view.buffer().index()],
                        file_name,
                        None,
                        false,
                    )
                    .expect("Couldn't load normal");
                    texture
                }
                // TODO: parse and pass the mime_type so that the img lib does't have to guess
                gltf::image::Source::Uri { uri, mime_type: _ } => {
                    let texture = load_texture(uri, false, device, queue, None).await?;
                    texture
                }
            }
        } else {
            Texture::create_default_normal_map(2, 2, device, queue)
        };
        let name = format!("{}.gltf", file_name);
        let name = name.as_str();
        let layout = &diffuse_normal_layout(device);
        materials.push(model::Material::new(
            device,
            name,
            diffuse_texture,
            normal_texture,
            layout,
        ));
    }

    let mut models = Vec::new();

    for scene in gltf.scenes() {
        for node in scene.nodes() {
            let model = to_scene_node(node, &buffer_data, device, &materials, &animations);
            models.push(model);
        }
    }

    let root_node = if models.len() == 1 {
        models.into_iter().next().unwrap()
    } else {
        let mut root_node = ContainerNode::new(0, Vec::new());
        root_node.children = models;
        Box::new(root_node)
    };

    Ok(root_node)
}
