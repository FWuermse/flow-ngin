//! Model loading from caller-owned files, including browser file selections.
use super::{default_material, mesh, texture::diffuse_normal_layout};
use crate::{
    data_structures::{
        model::{Material, Model},
        scene_graph::{ContainerNode, SceneNode, to_scene_node},
        texture::Texture,
    },
    pick::PickId,
};
use anyhow::{Context, Result, anyhow, bail, ensure};
use base64::Engine;
use std::{
    collections::HashMap,
    io::{BufReader, Cursor},
};

pub type AssetFiles = HashMap<String, Vec<u8>>;

fn decode_uri(uri: &str) -> Result<Vec<u8>> {
    if let Some(data) = uri.strip_prefix("data:") {
        let (meta, value) = data
            .split_once(',')
            .ok_or_else(|| anyhow!("Invalid data URI"))?;
        if meta.ends_with(";base64") {
            return Ok(base64::engine::general_purpose::STANDARD.decode(value)?);
        }
        return percent_decode(value);
    }
    bail!("Not an embedded URI")
}
fn percent_decode(s: &str) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let a = bytes.next().ok_or_else(|| anyhow!("Invalid escaped URI"))?;
            let b = bytes.next().ok_or_else(|| anyhow!("Invalid escaped URI"))?;
            out.push(u8::from_str_radix(std::str::from_utf8(&[a, b])?, 16)?);
        } else {
            out.push(b);
        }
    }
    Ok(out)
}
fn resolve(base: &str, uri: &str) -> Result<String> {
    let uri = String::from_utf8(percent_decode(uri)?)?.replace('\\', "/");
    ensure!(
        !uri.starts_with('/') && !uri.contains(':'),
        "Only selected local resources are supported: {uri}"
    );
    let parent = base.rsplit_once('/').map_or("", |(p, _)| p);
    let joined = format!("{parent}/{uri}");
    let mut parts = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                ensure!(
                    parts.pop().is_some(),
                    "Resource escapes selected folder: {uri}"
                );
            }
            p => parts.push(p),
        }
    }
    Ok(parts.join("/"))
}
fn bytes(files: &AssetFiles, base: &str, uri: &str) -> Result<Vec<u8>> {
    if uri.starts_with("data:") {
        return decode_uri(uri);
    }
    let path = resolve(base, uri)?;
    if let Some(data) = files.get(&path) {
        return Ok(data.clone());
    }
    // Multi-file browser selection has filenames but no directories. Only accept
    // an unambiguous basename; folder import retains the full relative paths.
    let name = path.rsplit('/').next().unwrap_or(&path);
    let mut matches = files
        .iter()
        .filter(|(p, _)| p.rsplit('/').next() == Some(name));
    match (matches.next(), matches.next()) {
        (Some((_, data)), None) => Ok(data.clone()),
        _ => bail!("Missing or ambiguous resource '{path}'; select its containing folder"),
    }
}
/// Load an OBJ and its selected MTL/texture dependencies without filesystem access.
pub fn load_model_obj_from_files(
    entry: &str,
    files: &AssetFiles,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<Model> {
    load_obj_files(entry, files, device, queue, true)
}

/// Load only OBJ geometry, ignoring material libraries and texture dependencies.
/// Useful for untextured overlays such as blueprint guides.
pub fn load_obj_geometry_from_files(
    entry: &str,
    files: &AssetFiles,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<Model> {
    load_obj_files(entry, files, device, queue, false)
}

fn load_obj_files(
    entry: &str,
    files: &AssetFiles,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    textured: bool,
) -> Result<Model> {
    let data = files
        .get(entry)
        .with_context(|| format!("Missing OBJ {entry}"))?;
    let material_base = std::cell::RefCell::new(HashMap::new());
    let (models, materials) = tobj::load_obj_buf(
        &mut BufReader::new(Cursor::new(data)),
        &tobj::LoadOptions {
            triangulate: true,
            single_index: true,
            ..Default::default()
        },
        |path| {
            if !textured {
                return Ok(Default::default());
            }
            let name = path.to_string_lossy();
            let resolved = resolve(entry, &name).map_err(|_| tobj::LoadError::OpenFileFailed)?;
            let data = bytes(files, entry, &name).map_err(|_| tobj::LoadError::OpenFileFailed)?;
            let result = tobj::load_mtl_buf(&mut BufReader::new(Cursor::new(data)))?;
            for mat in &result.0 {
                material_base
                    .borrow_mut()
                    .insert(mat.name.clone(), resolved.clone());
            }
            Ok(result)
        },
    )?;
    let material_base = material_base.into_inner();
    let layout = diffuse_normal_layout(device);
    let mut gpu_materials = Vec::new();
    for mat in materials? {
        let base = material_base.get(&mat.name).map_or(entry, String::as_str);
        let diffuse = match mat.diffuse_texture.as_deref().filter(|s| !s.is_empty()) {
            Some(uri) => {
                Texture::from_bytes(device, queue, &bytes(files, base, uri)?, uri, None, false)?
            }
            None => {
                let c = mat.diffuse.unwrap_or([0.8; 3]);
                Texture::from_color(
                    [
                        (c[0] * 255.) as u8,
                        (c[1] * 255.) as u8,
                        (c[2] * 255.) as u8,
                        255,
                    ],
                    device,
                    queue,
                )
            }
        };
        let normal = match mat.normal_texture.as_deref().filter(|s| !s.is_empty()) {
            Some(uri) => {
                Texture::from_bytes(device, queue, &bytes(files, base, uri)?, uri, None, true)?
            }
            None => Texture::create_default_normal_map(1, 1, device, queue),
        };
        gpu_materials.push(Material::new(device, &mat.name, diffuse, normal, &layout)?);
    }
    let fallback = gpu_materials.len();
    gpu_materials.push(default_material(device, queue)?);
    let mut meshes = mesh::load_meshes(&models, entry, device)
        .into_iter()
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (mesh, source) in meshes.iter_mut().zip(&models) {
        mesh.material = source.mesh.material_id.unwrap_or(fallback);
        ensure!(
            mesh.material < gpu_materials.len(),
            "Invalid OBJ material index"
        );
    }
    ensure!(!meshes.is_empty(), "OBJ contains no meshes");
    Ok(Model {
        meshes,
        materials: gpu_materials,
    })
}

/// Load a static glTF/GLB hierarchy from selected files and embedded resources.
pub fn load_model_gltf_from_files(
    id: impl Into<PickId>,
    entry: &str,
    files: &AssetFiles,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<Box<dyn SceneNode>> {
    load_gltf_files(id.into(), entry, files, device, queue, true)
}

/// Load glTF/GLB geometry and node transforms without image dependencies.
/// External geometry buffers must still be present in `files`.
pub fn load_gltf_geometry_from_files(
    id: impl Into<PickId>,
    entry: &str,
    files: &AssetFiles,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<Box<dyn SceneNode>> {
    load_gltf_files(id.into(), entry, files, device, queue, false)
}

fn load_gltf_files(
    id: PickId,
    entry: &str,
    files: &AssetFiles,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    textured: bool,
) -> Result<Box<dyn SceneNode>> {
    let gltf = gltf::Gltf::from_slice(
        files
            .get(entry)
            .with_context(|| format!("Missing glTF {entry}"))?,
    )?;
    let mut buffers = Vec::new();
    for buffer in gltf.buffers() {
        let data = match buffer.source() {
            gltf::buffer::Source::Bin => gltf
                .blob
                .clone()
                .ok_or_else(|| anyhow!("Missing GLB buffer"))?,
            gltf::buffer::Source::Uri(uri) => bytes(files, entry, uri)?,
        };
        ensure!(data.len() >= buffer.length(), "Truncated glTF buffer");
        buffers.push(data);
    }
    for view in gltf.views() {
        ensure!(
            view.offset()
                .checked_add(view.length())
                .is_some_and(|end| end <= buffers[view.buffer().index()].len()),
            "Invalid glTF buffer view"
        );
    }
    for accessor in gltf.accessors() {
        if let Some(view) = accessor.view() {
            let stride = view.stride().unwrap_or(accessor.size());
            let end = accessor
                .count()
                .saturating_sub(1)
                .checked_mul(stride)
                .and_then(|n| n.checked_add(accessor.offset()))
                .and_then(|n| n.checked_add(accessor.size()));
            ensure!(
                accessor.count() > 0
                    && stride >= accessor.size()
                    && end.is_some_and(|n| n <= view.length()),
                "glTF accessor exceeds its buffer view"
            );
        }
    }
    for mesh in gltf.meshes() {
        for p in mesh.primitives() {
            ensure!(
                p.mode() == gltf::mesh::Mode::Triangles,
                "Only triangle glTF meshes are supported"
            );
            let reader = p.reader(|b| Some(buffers[b.index()].as_slice()));
            let n = reader
                .read_positions()
                .ok_or_else(|| anyhow!("Missing vertex positions"))?
                .count();
            ensure!(n > 0, "Empty glTF primitive");
            if let Some(indices) = reader.read_indices() {
                let indices: Vec<_> = indices.into_u32().collect();
                ensure!(
                    indices.len() % 3 == 0 && indices.iter().all(|&i| (i as usize) < n),
                    "Invalid glTF indices"
                );
            } else {
                ensure!(n % 3 == 0, "Invalid triangle vertex count");
            }
            for (_, accessor) in p.attributes() {
                ensure!(accessor.count() == n, "Mismatched glTF attributes");
            }
        }
    }
    let image = |source: gltf::Image, normal| -> Result<Texture> {
        let (data, format) = match source.source() {
            gltf::image::Source::Uri { uri, mime_type } => (bytes(files, entry, uri)?, mime_type),
            gltf::image::Source::View { view, mime_type } => (
                buffers[view.buffer().index()][view.offset()..view.offset() + view.length()]
                    .to_vec(),
                Some(mime_type),
            ),
        };
        Texture::from_bytes(
            device,
            queue,
            &data,
            entry,
            format.and_then(|m| m.rsplit('/').next()),
            normal,
        )
    };
    let layout = diffuse_normal_layout(device);
    let mut materials = Vec::new();
    for mat in gltf.materials() {
        if !textured {
            materials.push(default_material(device, queue)?);
            continue;
        }
        let pbr = mat.pbr_metallic_roughness();
        let diffuse = match pbr.base_color_texture() {
            Some(t) => image(t.texture().source(), false)?,
            None => Texture::from_color(
                pbr.base_color_factor().map(|c| (c * 255.).round() as u8),
                device,
                queue,
            ),
        };
        let normal = match mat.normal_texture() {
            Some(t) => image(t.texture().source(), true)?,
            None => Texture::create_default_normal_map(1, 1, device, queue),
        };
        materials.push(Material::new(
            device,
            mat.name().unwrap_or("Material"),
            diffuse,
            normal,
            &layout,
        )?);
    }
    materials.push(default_material(device, queue)?);
    let scene = gltf
        .default_scene()
        .or_else(|| gltf.scenes().next())
        .ok_or_else(|| anyhow!("glTF contains no scene"))?;
    let mut root = ContainerNode::new(1, Vec::new());
    for node in scene.nodes() {
        root.add_child(to_scene_node(
            id,
            node,
            &buffers,
            device,
            &materials,
            &HashMap::new(),
        ));
    }
    root.update_world_transform_all();
    ensure!(
        !root.get_renders().is_empty(),
        "glTF scene contains no meshes"
    );
    Ok(Box::new(root))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_resources_are_relative_and_unambiguous() {
        let files = HashMap::from([
            ("models/textures/a.bin".into(), vec![1]),
            ("other/a.bin".into(), vec![2]),
        ]);
        assert_eq!(
            bytes(&files, "models/sub/model.gltf", "../textures/a.bin").unwrap(),
            vec![1]
        );
        assert!(bytes(&files, "missing/model.gltf", "a.bin").is_err());
        assert!(bytes(&files, "model.gltf", "../a.bin").is_err());
        assert_eq!(
            bytes(
                &files,
                "model.gltf",
                "data:application/octet-stream;base64,AQID"
            )
            .unwrap(),
            vec![1, 2, 3]
        );
    }
}
