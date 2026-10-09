//! Fortress Flow assets use paths relative to /assets/, including MTL references.
//! Download dependencies into a flat bundle for the shared native/WASM OBJ loader.
use crate::document::BlockType;
use anyhow::{Context, Result, ensure};
use flow_ngin::resources::AssetFiles;

pub const BASE_URL: &str = "https://play.fortressflow.com/assets/";
pub const ATLAS_PATH: &str = "atlas.png";
pub const ENTRY: &str = "model.obj";

impl BlockType {
    pub fn model_path(self) -> Option<&'static str> {
        Some(match self {
            Self::Stone => "resources/cube.obj",
            Self::Log => "resources/log.obj",
            Self::Wood => "resources/half_slab.obj",
            Self::Clay => "resources/roof_45.obj",
            Self::Wheat => "resources/wheat.obj",
            Self::Copper => "resources/copper.obj",
            Self::Iron => "resources/iron.obj",
            Self::Gold => return None,
        })
    }

    /// Zero-based, row-major slots in the 8 × 8 atlas.
    pub fn icon_slot(self) -> u8 {
        match self {
            Self::Stone => 28,
            Self::Clay => 37,
            Self::Wood => 29,
            Self::Log => 30,
            Self::Wheat => 36,
            Self::Copper | Self::Iron | Self::Gold => 38,
        }
    }
}

pub async fn download(client: &reqwest::Client, path: &str) -> Result<Vec<u8>> {
    let url = reqwest::Url::parse(BASE_URL)?.join(path)?;
    let response = client
        .get(url.clone())
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .with_context(|| format!("Download {url}"))?
        .error_for_status()
        .with_context(|| format!("Download {url}"))?;
    Ok(response.bytes().await?.to_vec())
}

fn directive(line: &str) -> (&str, &str) {
    let line = line.split('#').next().unwrap_or("").trim();
    line.split_once(char::is_whitespace)
        .map_or((line, ""), |(key, value)| (key, value.trim()))
}

pub async fn model_files(client: &reqwest::Client, path: &str) -> Result<AssetFiles> {
    let source = String::from_utf8(download(client, path).await?)?;
    let mut files = AssetFiles::new();
    let mut obj = String::new();
    let mut material_names = Vec::new();
    for line in source.lines() {
        let (key, value) = directive(line);
        if key == "mtllib" {
            // The supplied exports use one material filename per mtllib line.
            let material = String::from_utf8(download(client, value).await?)?;
            let mut mtl = String::new();
            for line in material.lines() {
                let (key, value) = directive(line);
                if key == "newmtl" {
                    material_names.push(value.to_owned());
                }
                if matches!(key, "map_Kd" | "map_Bump" | "bump" | "norm") {
                    ensure!(
                        !value.starts_with('-'),
                        "Unsupported texture options: {line}"
                    );
                    let name = format!("texture-{}", files.len());
                    files.insert(name.clone(), download(client, value).await?);
                    mtl.push_str(&format!("{key} {name}\n"));
                } else {
                    mtl.push_str(line);
                    mtl.push('\n');
                }
            }
            let name = format!("material-{}.mtl", files.len());
            files.insert(name.clone(), mtl.into_bytes());
            obj.push_str(&format!("mtllib {name}\n"));
        } else {
            obj.push_str(line);
            obj.push('\n');
        }
    }
    // Some supplied exports (notably log.obj) declare one material library but
    // omit usemtl. Bind its sole material so diffuse and normal maps are used.
    if material_names.len() == 1 {
        obj = assign_sole_material(&obj, &material_names[0]);
    }
    files.insert(ENTRY.into(), ground_model(&obj)?.into_bytes());
    Ok(files)
}

fn assign_sole_material(source: &str, material: &str) -> String {
    if source.lines().any(|line| directive(line).0 == "usemtl") {
        return source.to_owned();
    }
    let mut result = String::new();
    let mut assigned = false;
    for line in source.lines() {
        if !assigned && directive(line).0 == "f" {
            result.push_str(&format!("usemtl {material}\n"));
            assigned = true;
        }
        result.push_str(line);
        result.push('\n');
    }
    result
}

/// Preserve dimensions, but put the bottom at y=0 as expected by editor placement.
fn ground_model(source: &str) -> Result<String> {
    let mut bottom = f32::INFINITY;
    for line in source.lines() {
        let (key, value) = directive(line);
        if key == "v" {
            let coords = value.split_whitespace().collect::<Vec<_>>();
            ensure!(coords.len() >= 3, "Invalid OBJ vertex");
            let y = coords[1].parse::<f32>()?;
            ensure!(y.is_finite(), "Invalid OBJ vertex height");
            bottom = bottom.min(y);
        }
    }
    ensure!(bottom.is_finite(), "OBJ contains no vertices");
    let mut result = String::new();
    for line in source.lines() {
        let (key, value) = directive(line);
        if key == "v" {
            let mut coords = value
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            coords[1] = (coords[1].parse::<f32>()? - bottom).to_string();
            result.push_str(&format!("v {}\n", coords.join(" ")));
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grounding_preserves_shape_and_texture_coordinates() {
        let obj = "v -0.5 -0.5 0.5\nv 0.5 0 0.5\nvt 0.2 0.3\nf 1/1 2/1 1/1\n";
        assert_eq!(
            ground_model(obj).unwrap(),
            "v -0.5 0 0.5\nv 0.5 0.5 0.5\nvt 0.2 0.3\nf 1/1 2/1 1/1\n"
        );
        assert!(ground_model("v 0 NaN 0").is_err());
        assert!(ground_model("# empty").is_err());
    }

    #[test]
    fn missing_material_assignment_is_repaired_without_overriding_explicit_materials() {
        let source = "mtllib log.mtl\nv 0 0 0\nf 1 1 1\n";
        assert_eq!(
            assign_sole_material(source, "Bark"),
            "mtllib log.mtl\nv 0 0 0\nusemtl Bark\nf 1 1 1\n"
        );
        let explicit = "usemtl Other\nf 1 1 1\n";
        assert_eq!(assign_sole_material(explicit, "Bark"), explicit);
    }

    /// Opt-in verification of the actual hosted files, materials, textures and atlas.
    #[test]
    #[ignore = "requires network access and a GPU adapter"]
    #[cfg(not(target_arch = "wasm32"))]
    fn hosted_assets_load_on_gpu() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: wgpu::Backends::PRIMARY,
                flags: Default::default(),
                memory_budget_thresholds: Default::default(),
                backend_options: Default::default(),
                display: None,
            });
            let adapter = instance.request_adapter(&Default::default()).await.unwrap();
            let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
            let client = reqwest::Client::new();
            for kind in BlockType::ALL {
                if let Some(path) = kind.model_path() {
                    let files = model_files(&client, path)
                        .await
                        .unwrap_or_else(|e| panic!("{}: {e:#}", kind.label()));
                    let model = flow_ngin::resources::load_model_obj_from_files(
                        ENTRY, &files, &device, &queue,
                    )
                    .unwrap_or_else(|e| panic!("{}: {e:#}", kind.label()));
                    assert!(!model.meshes.is_empty());
                    if kind == BlockType::Log {
                        assert!(
                            model.meshes.iter().all(|mesh| mesh.material == 0),
                            "Log must use its bark material, not the untextured fallback"
                        );
                        assert!(
                            files
                                .values()
                                .filter_map(|bytes| std::str::from_utf8(bytes).ok())
                                .any(|text| text.contains("map_Kd texture-")
                                    && text.contains("map_Bump texture-"))
                        );
                    }
                    eprintln!("{}: {} meshes loaded", kind.label(), model.meshes.len());
                }
            }
            let atlas = download(&client, ATLAS_PATH).await.unwrap();
            flow_ngin::ui::image::Atlas::from_bytes(&device, &queue, &atlas, 8, 8).unwrap();
        });
    }
}
