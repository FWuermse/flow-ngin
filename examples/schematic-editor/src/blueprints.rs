use crate::document::{Blueprint, Document, InstanceData};
use anyhow::{Context, Result, ensure};
use flow_ngin::{
    context::InitContext,
    data_structures::scene_graph::{ContainerNode, ModelNode, SceneNode},
    resources::{AssetFiles, load_gltf_geometry_from_files, load_obj_geometry_from_files},
};
use std::{collections::HashMap, sync::Arc};
pub struct GuideGpu {
    pub root: Box<dyn SceneNode>,
    pub transform: InstanceData,
}
pub type GuideCache = HashMap<u64, GuideGpu>;
pub fn load(guide: &Blueprint, gpu: &InitContext) -> Result<GuideGpu> {
    let extension = guide
        .entry
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let child: Box<dyn SceneNode> = match extension.as_str() {
        "obj" => Box::new(ModelNode::from_model(
            1,
            0u32,
            &gpu.device,
            load_obj_geometry_from_files(&guide.entry, &guide.files, &gpu.device, &gpu.queue)?,
            Vec::new(),
        )),
        "gltf" | "glb" => load_gltf_geometry_from_files(
            0u32,
            &guide.entry,
            &guide.files,
            &gpu.device,
            &gpu.queue,
        )?,
        _ => anyhow::bail!("Unsupported blueprint format"),
    };
    let mut root = ContainerNode::new(1, Vec::new());
    root.add_child(child);
    root.set_local_transform(0, guide.transform.instance());
    root.update_world_transform_all();
    root.write_to_buffers(&gpu.queue, &gpu.device);
    ensure!(
        root.get_renders()
            .iter()
            .any(|r| r.amount > 0 && r.model.meshes.iter().any(|m| m.num_elements > 0)),
        "Blueprint contains no renderable geometry"
    );
    Ok(GuideGpu {
        root: Box::new(root),
        transform: guide.transform,
    })
}
pub fn import(
    files: AssetFiles,
    next: u64,
    gpu: &InitContext,
) -> Result<(Vec<Blueprint>, GuideCache)> {
    let files = Arc::new(files);
    let mut entries = files
        .keys()
        .filter(|name| {
            matches!(
                name.rsplit('.')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .as_str(),
                "obj" | "gltf" | "glb"
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    entries.sort();
    ensure!(!entries.is_empty(), "No OBJ, glTF or GLB models selected");
    let mut guides = Vec::new();
    let mut cache = HashMap::new();
    for (i, entry) in entries.into_iter().enumerate() {
        let guide = Blueprint {
            id: next + i as u64,
            name: entry.rsplit('/').next().unwrap().to_owned(),
            entry,
            files: files.clone(),
            transform: InstanceData::default(),
            opacity: 0.35,
            visible: true,
        };
        cache.insert(
            guide.id,
            load(&guide, gpu).with_context(|| format!("Import failed: {}", guide.name))?,
        );
        guides.push(guide);
    }
    Ok((guides, cache))
}
pub fn load_document(document: &Document, gpu: &InitContext) -> Result<GuideCache> {
    document
        .blueprints
        .iter()
        .map(|guide| Ok((guide.id, load(guide, gpu)?)))
        .collect()
}
