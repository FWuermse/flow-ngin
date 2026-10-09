use crate::{
    codec,
    document::{Blueprint, Document, InstanceData},
    editor::Settings,
};
use anyhow::{Context, Result, ensure};
use flow_ngin::resources::AssetFiles;
use serde::{Deserialize, Serialize};
use std::{
    io::{Cursor, Read, Write},
    sync::Arc,
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};
const MAX_BYTES: u64 = 512 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u32,
    settings: Settings,
    blueprints: Vec<GuideEntry>,
}
#[derive(Serialize, Deserialize)]
struct GuideEntry {
    id: u64,
    asset_set: u64,
    name: String,
    entry: String,
    instance: InstanceData,
    opacity: f32,
    visible: bool,
}
fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['\\', ':'])
        && path.split('/').all(|p| !matches!(p, ".." | "." | ""))
}
pub fn encode(doc: &Document, settings: Settings) -> Result<Vec<u8>> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("schematic.bin", options)?;
    zip.write_all(&codec::encode(&doc.schematic())?)?;
    let mut asset_sets: Vec<(u64, Arc<AssetFiles>)> = Vec::new();
    let mut entries = Vec::new();
    for guide in &doc.blueprints {
        let asset_set = if let Some((id, _)) = asset_sets
            .iter()
            .find(|(_, files)| Arc::ptr_eq(files, &guide.files))
        {
            *id
        } else {
            asset_sets.push((guide.id, guide.files.clone()));
            guide.id
        };
        entries.push(GuideEntry {
            id: guide.id,
            asset_set,
            name: guide.name.clone(),
            entry: guide.entry.clone(),
            instance: guide.transform,
            opacity: guide.opacity,
            visible: guide.visible,
        });
    }
    let manifest = Manifest {
        version: 1,
        settings,
        blueprints: entries,
    };
    zip.start_file("project.json", options)?;
    zip.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
    for (id, assets) in asset_sets {
        let mut files: Vec<_> = assets.iter().collect();
        files.sort_by_key(|(name, _)| *name);
        for (path, bytes) in files {
            ensure!(safe_path(path), "Invalid blueprint path: {path}");
            zip.start_file(format!("blueprints/{id}/{path}"), options)?;
            zip.write_all(bytes)?;
        }
    }
    Ok(zip.finish()?.into_inner())
}
pub fn decode(bytes: &[u8]) -> Result<(Document, Settings)> {
    let mut zip = ZipArchive::new(Cursor::new(bytes))?;
    ensure!(zip.len() <= 10000, "Too many project files");
    let mut files = AssetFiles::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let file = zip.by_index(i)?;
        if file.is_dir() {
            continue;
        }
        ensure!(safe_path(file.name()), "Invalid project entry path");
        total = total
            .checked_add(file.size())
            .context("Project too large")?;
        ensure!(total <= MAX_BYTES, "Project exceeds 512 MiB");
        let name = file.name().to_owned();
        let mut data = Vec::new();
        file.take(MAX_BYTES + 1).read_to_end(&mut data)?;
        ensure!(data.len() as u64 <= MAX_BYTES, "Project file too large");
        ensure!(
            files.insert(name, data).is_none(),
            "Duplicate project entry"
        );
    }
    let manifest: Manifest =
        serde_json::from_slice(files.get("project.json").context("Missing project.json")?)?;
    ensure!(manifest.version == 1, "Unsupported project version");
    let mut doc = Document::from_schematic(codec::decode(
        files
            .get("schematic.bin")
            .context("Missing schematic.bin")?,
    )?)?;
    let mut used = std::collections::HashSet::new();
    let mut next = doc.next_id();
    let mut asset_sets = std::collections::HashMap::<u64, Arc<AssetFiles>>::new();
    for entry in manifest.blueprints {
        ensure!(used.insert(entry.id), "Duplicate guide ID");
        ensure!(
            entry.instance.valid()
                && (entry.instance.scale[0] - entry.instance.scale[1]).abs() < 1e-5
                && (entry.instance.scale[0] - entry.instance.scale[2]).abs() < 1e-5,
            "Invalid guide transform"
        );
        ensure!(
            entry.opacity.is_finite() && (0.0..=1.0).contains(&entry.opacity),
            "Invalid guide opacity"
        );
        let assets = asset_sets
            .entry(entry.asset_set)
            .or_insert_with(|| {
                let prefix = format!("blueprints/{}/", entry.asset_set);
                Arc::new(
                    files
                        .iter()
                        .filter_map(|(name, data)| {
                            name.strip_prefix(&prefix)
                                .map(|n| (n.to_owned(), data.clone()))
                        })
                        .collect::<AssetFiles>(),
                )
            })
            .clone();
        ensure!(
            assets.contains_key(&entry.entry),
            "Missing guide entry file"
        );
        doc.blueprints.push(Blueprint {
            id: next,
            name: entry.name,
            entry: entry.entry,
            files: assets,
            transform: entry.instance,
            opacity: entry.opacity,
            visible: entry.visible,
        });
        next += 1;
    }
    Ok((doc, manifest.settings))
}
pub fn open(name: &str, bytes: &[u8]) -> Result<(Document, Settings)> {
    if name.to_ascii_lowercase().ends_with(".zip") {
        decode(bytes)
    } else {
        Ok((
            Document::from_schematic(codec::decode(bytes)?)?,
            Settings::default(),
        ))
    }
}
