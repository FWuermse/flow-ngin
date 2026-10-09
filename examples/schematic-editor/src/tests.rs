use crate::{codec, document::*, editor::*, geometry, project};
use cgmath::{Deg, InnerSpace, Point3, Quaternion, Rotation3, Vector3};
use std::{collections::HashMap, sync::Arc};
fn block(id: u64, p: [f32; 3]) -> Block {
    Block {
        id,
        kind: BlockType::Stone,
        transform: InstanceData {
            position: p,
            ..Default::default()
        },
    }
}
fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-4, "{a} != {b}");
}

#[test]
fn binary_is_compatible_with_supplied_async_parser() {
    // Independent reader following the user's exact wire protocol.
    use futures::io::{AsyncRead, AsyncReadExt};
    async fn read<const N: usize, R: AsyncRead + Unpin>(r: &mut R) -> [u8; N] {
        let mut out = [0; N];
        r.read_exact(&mut out).await.unwrap();
        out
    }
    futures::executor::block_on(async {
        let mut transform = InstanceData {
            position: [-2., 3.25, 7.],
            scale: [2., 1., 0.5],
            ..Default::default()
        };
        transform.set_rotation(Quaternion::from_axis_angle(Vector3::unit_y(), Deg(35.)));
        let source = SchematicData {
            version: 1,
            blocks: HashMap::from([
                ("stone".into(), vec![transform]),
                ("gold".into(), vec![InstanceData::default()]),
            ]),
        };
        let bytes = codec::encode(&source).unwrap();
        let mut r = &bytes[..];
        assert_eq!(u32::from_le_bytes(read(&mut r).await), 1);
        let count = u32::from_le_bytes(read(&mut r).await);
        let mut blocks = HashMap::new();
        for _ in 0..count {
            let length = u16::from_le_bytes(read(&mut r).await) as usize;
            let mut name = vec![0; length];
            r.read_exact(&mut name).await.unwrap();
            let count = u32::from_le_bytes(read(&mut r).await);
            let mut instances = Vec::new();
            for _ in 0..count {
                let mut position = [0.; 3];
                let mut rotation = [0.; 4];
                let mut scale = [0.; 3];
                for value in position.iter_mut().chain(&mut rotation).chain(&mut scale) {
                    *value = f32::from_le_bytes(read(&mut r).await);
                }
                instances.push(InstanceData {
                    position,
                    rotation,
                    scale,
                });
            }
            blocks.insert(String::from_utf8(name).unwrap(), instances);
        }
        assert_eq!(blocks, source.blocks);
        assert!(r.is_empty());
        assert_eq!(codec::decode(&bytes).unwrap().blocks, source.blocks);
    });
}
#[test]
fn malformed_binary_fails_without_partial_document() {
    let data = codec::encode(&Document::default().schematic()).unwrap();
    assert_eq!(codec::decode(&data).unwrap().blocks.len(), 0);
    for n in 0..data.len() {
        assert!(codec::decode(&data[..n]).is_err());
    }
    let mut version = data.clone();
    version[0] = 2;
    assert!(codec::decode(&version).is_err());
    let mut count = data.clone();
    count[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(codec::decode(&count).is_err());
    let mut trailing = data;
    trailing.push(0);
    assert!(codec::decode(&trailing).is_err());
    let unknown = SchematicData {
        version: 1,
        blocks: HashMap::from([("STONE".into(), vec![InstanceData::default()])]),
    };
    assert!(Document::from_schematic(unknown).is_err());
}
#[test]
fn paint_stacks_continuously_and_escape_drops_only_preview() {
    let mut e = Editor::default();
    e.paint(BlockType::Stone);
    e.update_position(Some([-1.2, 2.3]));
    assert_eq!(
        e.preview.as_ref().unwrap().blocks[0].transform.position,
        [-1., 0., 2.]
    );
    assert!(e.commit_preview());
    e.update_position(Some([-1.2, 2.3]));
    close(
        e.preview.as_ref().unwrap().blocks[0].transform.position[1],
        1.,
    );
    assert!(e.commit_preview());
    e.update_position(Some([-1.2, 2.3]));
    close(
        e.preview.as_ref().unwrap().blocks[0].transform.position[1],
        2.,
    );
    e.escape();
    assert_eq!(e.document.blocks.len(), 2);
    assert!(e.preview.is_none());
    assert!(e.selected.is_empty());
}
#[test]
fn manual_height_blocks_overlap_and_grid_toggle_does_not_change_document() {
    let mut e = Editor::default();
    e.document.blocks.push(block(20, [0., 0., 0.]));
    e.next_id = 21;
    e.settings.manual_height = true;
    e.paint(BlockType::Wood);
    e.update_position(Some([0., 0.]));
    assert!(!e.preview.as_ref().unwrap().valid);
    assert!(!e.commit_preview());
    e.height(1.);
    assert!(e.preview.as_ref().unwrap().valid);
    assert!(e.commit_preview());
    let before = e.document.blocks.clone();
    e.settings.grid = false;
    e.update_position(Some([-2.2, -3.3]));
    close(
        e.preview.as_ref().unwrap().blocks[0].transform.center().x,
        -2.2,
    );
    e.height(1.);
    close(
        e.preview.as_ref().unwrap().blocks[0].transform.position[1],
        1.25,
    );
    assert_eq!(e.document.blocks, before);
}
#[test]
fn enabling_snap_resets_free_rotation_and_move_baseline() {
    let mut e = Editor::default();
    e.document.blocks.push(block(1, [0., 0., 0.]));
    e.selected.insert(1);
    let original = e.document.blocks.clone();
    e.begin_transform(false, Some([0., 0.]));
    e.set_grid(false);
    e.axis = 0;
    e.rotate(2.);
    e.update_position(Some([2.2, 3.3]));
    e.set_grid(true);
    let p = e.preview.as_ref().unwrap();
    assert_eq!(p.blocks[0].transform.rotation, [0., 0., 0., 1.]);
    assert_eq!(p.movement_base, p.blocks);
    assert!(p.cursor_anchor.is_none());
    close(p.blocks[0].transform.position[0], 2.);
    close(p.blocks[0].transform.position[2], 3.);
    assert_eq!(e.document.blocks, original);
    e.update_position(Some([2.2, 3.3]));
    e.update_position(Some([3.2, 3.3]));
    let p = e.preview.as_ref().unwrap();
    assert_eq!(p.blocks[0].transform.rotation, [0., 0., 0., 1.]);
    close(p.blocks[0].transform.position[0], 3.);
    assert!(e.commit_preview());
    e.undo(false);
    assert_eq!(e.document.blocks, original);
}

#[test]
fn snap_reset_allows_clean_quarter_turns_for_paint() {
    let mut e = Editor::default();
    e.paint(BlockType::Log);
    e.update_position(Some([0., 0.]));
    e.set_grid(false);
    e.rotate(1.);
    e.set_grid(true);
    assert_eq!(
        e.preview.as_ref().unwrap().blocks[0].transform.rotation,
        [0., 0., 0., 1.]
    );
    e.rotate(1.);
    let expected = Quaternion::from_axis_angle(Vector3::unit_y(), Deg(90.));
    let actual = e.preview.as_ref().unwrap().blocks[0].transform.rotation();
    close(actual.s, expected.s);
    close(actual.v.y, expected.v.y);
    // Enabling an already enabled grid must retain deliberate snapped rotations.
    e.set_grid(true);
    assert_eq!(
        e.preview.as_ref().unwrap().blocks[0].transform.rotation(),
        actual
    );
}

#[test]
fn touching_faces_are_valid_and_rotated_intersections_rejected() {
    let a = InstanceData::default();
    let mut b = InstanceData {
        position: [1., 0., 0.],
        ..a
    };
    assert!(!geometry::overlaps(a, b));
    b.position[0] = 0.9;
    assert!(geometry::overlaps(a, b));
    b.set_rotation(Quaternion::from_axis_angle(Vector3::unit_y(), Deg(45.)));
    b.set_center(Vector3::new(1.1, 0.5, 0.));
    assert!(geometry::overlaps(a, b));
    b.set_center(Vector3::new(1.3, 0.5, 0.));
    assert!(!geometry::overlaps(a, b));
}
#[test]
fn center_rotation_preserves_wall_and_half_grid_exception() {
    let mut e = Editor::default();
    e.document.blocks = vec![block(1, [0., 0., 0.]), block(2, [1., 0., 0.])];
    e.next_id = 3;
    e.selected.extend([1, 2]);
    e.begin_transform(true, None);
    e.rotate(1.);
    let p = e.preview.as_ref().unwrap();
    let centers = p
        .blocks
        .iter()
        .map(|b| b.transform.center())
        .collect::<Vec<_>>();
    close(centers[0].x, 0.5);
    close(centers[1].x, 0.5);
    close((centers[1] - centers[0]).magnitude(), 1.);
    close((centers[0] + centers[1]).x * 0.5, 0.5);
    assert!(p.valid);
    assert!(e.commit_preview());
    e.undo(false);
    assert_eq!(e.document.blocks[1].transform.position, [1., 0., 0.]);
    e.undo(true);
    close(e.document.blocks[0].transform.center().x, 0.5);
}
#[test]
fn move_cancel_copy_paste_delete_and_history_are_transactional() {
    let mut e = Editor::default();
    e.document.blocks = vec![block(1, [0., 0., 0.]), block(2, [1., 0., 0.])];
    e.next_id = 3;
    e.selected.extend([1, 2]);
    let original = e.document.blocks.clone();
    e.begin_transform(false, Some([0., 0.]));
    e.update_position(Some([3., 2.]));
    assert_eq!(e.document.blocks, original);
    e.escape();
    assert_eq!(e.document.blocks, original);
    e.selected.extend([1, 2]);
    e.copy();
    e.paste();
    e.update_position(Some([6., 0.]));
    assert!(e.commit_preview());
    assert_eq!(e.document.blocks.len(), 4);
    assert!(e.document.blocks[2].id > 2);
    e.delete_ids(&[3, 4]);
    assert_eq!(e.document.blocks.len(), 2);
    e.undo(false);
    assert_eq!(e.document.blocks.len(), 4);
    e.undo(false);
    assert_eq!(e.document.blocks, original);
    e.undo(true);
    assert_eq!(e.document.blocks.len(), 4);
}
#[test]
fn moving_group_does_not_collide_with_original_instances() {
    let mut e = Editor::default();
    e.document.blocks = vec![block(1, [0., 0., 0.])];
    e.selected.insert(1);
    e.begin_transform(false, Some([0., 0.]));
    e.update_position(Some([0., 0.]));
    assert!(e.preview.as_ref().unwrap().valid);
    close(
        e.preview.as_ref().unwrap().blocks[0].transform.position[1],
        0.,
    );
}
#[test]
fn rotated_ray_picks_nearest_block_and_projection_rejects_behind_camera() {
    let mut a = block(1, [0., 0., 0.]);
    a.transform
        .set_rotation(Quaternion::from_axis_angle(Vector3::unit_y(), Deg(45.)));
    let blocks = vec![block(2, [0., 0., -4.]), a];
    assert_eq!(
        geometry::pick(Point3::new(0., 0.5, 5.), Vector3::new(0., 0., -1.), &blocks),
        Some(1)
    );
    let camera = crate::input::Input::initial_camera();
    let projection = flow_ngin::camera::Projection::new(1000, 800, Deg(45.), 0.1, 1000.).unwrap();
    let vp = projection.calc_matrix() * camera.calc_matrix();
    assert!(geometry::project(Vector3::new(0., 0., 0.), vp, [1000., 800.]).is_some());
    assert!(geometry::project(Vector3::new(0., 30., 40.), vp, [1000., 800.]).is_none());
}
#[test]
fn bundle_preserves_original_files_and_guide_instance() {
    let originals = Arc::new(HashMap::from([
        ("model.obj".into(), b"original model bytes".to_vec()),
        ("textures/test.png".into(), vec![1, 2, 3, 4]),
    ]));
    let mut transform = InstanceData {
        position: [3., -2., 7.],
        scale: [2.5; 3],
        ..Default::default()
    };
    transform.set_rotation(Quaternion::from_axis_angle(Vector3::unit_x(), Deg(30.)));
    let doc = Document {
        blocks: vec![block(1, [0., 0., 0.])],
        blueprints: vec![Blueprint {
            id: 9,
            name: "Guide".into(),
            entry: "model.obj".into(),
            files: originals.clone(),
            transform,
            opacity: 0.25,
            visible: false,
        }],
    };
    let settings = Settings {
        grid: false,
        manual_height: true,
    };
    let bytes = project::encode(&doc, settings).unwrap();
    let (restored, prefs) = project::decode(&bytes).unwrap();
    assert_eq!(prefs, settings);
    assert_eq!(restored.schematic().blocks, doc.schematic().blocks);
    let guide = &restored.blueprints[0];
    assert_eq!(guide.files.as_ref(), originals.as_ref());
    assert_eq!(guide.transform, transform);
    assert_eq!(guide.opacity, 0.25);
    assert!(!guide.visible);
}
#[test]
fn savepoint_becomes_dirty_after_branching_history() {
    let mut doc = Document::default();
    let mut history = History::clean();
    history.commit(&mut doc, Edit::blocks(vec![], vec![block(1, [0.; 3])]));
    history.mark_saved();
    assert!(!history.dirty());
    history.undo(&mut doc);
    assert!(history.dirty());
    history.commit(&mut doc, Edit::blocks(vec![], vec![block(2, [1.; 3])]));
    assert!(history.dirty());
}

#[test]
fn bundle_shares_asset_sets_between_guides() {
    let files = Arc::new(HashMap::from([
        ("a.obj".into(), b"a".to_vec()),
        ("b.obj".into(), b"b".to_vec()),
    ]));
    let doc = Document {
        blocks: vec![],
        blueprints: ["a.obj", "b.obj"]
            .into_iter()
            .enumerate()
            .map(|(i, name)| Blueprint {
                id: i as u64 + 1,
                name: name.into(),
                entry: name.into(),
                files: files.clone(),
                transform: InstanceData::default(),
                opacity: 0.35,
                visible: true,
            })
            .collect(),
    };
    let bytes = project::encode(&doc, Settings::default()).unwrap();
    let zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    assert_eq!(zip.len(), 4);
    let (restored, _) = project::decode(&bytes).unwrap();
    assert!(Arc::ptr_eq(
        &restored.blueprints[0].files,
        &restored.blueprints[1].files
    ));
}

#[test]
fn selected_models_load_from_memory_and_preserve_parent_transforms() {
    use flow_ngin::{
        context::InitContext,
        resources::{AssetFiles, load_model_gltf_from_files, load_model_obj_from_files},
    };
    futures::executor::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            flags: Default::default(),
            memory_budget_thresholds: Default::default(),
            backend_options: Default::default(),
            display: None,
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("GPU adapter for import regression test");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        // Validate the lighting shaders on a real device, including their shared
        // ambient-light uniform, rather than relying only on Rust compilation.
        for source in [
            include_str!("../../../src/pipelines/block_shader.wgsl"),
            include_str!("../../../src/pipelines/transparent.wgsl"),
            include_str!("../../../src/pipelines/terrain8t.wgsl"),
        ] {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Editor lighting validation"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
        }
        let mut files = AssetFiles::from([
            (
                "models/test.obj".into(),
                b"mtllib materials/test.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nusemtl Color\nf 1 2 3\n"
                    .to_vec(),
            ),
            (
                "models/materials/test.mtl".into(),
                b"newmtl Color\nKd 0.2 0.4 0.6\nmap_Kd ../textures/color.png\n".to_vec(),
            ),
            (
                "models/textures/color.png".into(),
                include_bytes!("../../../assets/white-diffuse.png").to_vec(),
            ),
        ]);
        let obj = load_model_obj_from_files("models/test.obj", &files, &device, &queue).unwrap();
        assert_eq!(obj.meshes.len(), 1);
        assert_eq!(obj.materials.len(), 2);
        files.remove("models/textures/color.png");
        assert!(load_model_obj_from_files("models/test.obj", &files, &device, &queue).is_err());
        // A standalone OBJ guide must not require its missing MTL or images.
        let standalone = AssetFiles::from([(
            "standalone.obj".into(),
            b"mtllib missing.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nusemtl Missing\nf 1 2 3\n".to_vec(),
        )]);
        let init = InitContext {
            device: device.clone(),
            queue: queue.clone(),
        };
        let (guides, cache) = crate::blueprints::import(standalone, 10, &init).unwrap();
        assert_eq!(guides.len(), 1);
        let renders = cache[&10].root.get_renders();
        assert_eq!(renders.len(), 1);
        assert_eq!(renders[0].amount, 1);
        assert_eq!(renders[0].model.meshes[0].num_elements, 3);
        let mut bin = Vec::new();
        for f in [0f32, 0., 0., 1., 0., 0., 0., 1., 0.] {
            bin.extend(f.to_le_bytes());
        }
        let png = include_bytes!("../../../assets/white-diffuse.png");
        bin.extend(png);
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        let json = serde_json::json!({
            "asset":{"version":"2.0"},"buffers":[{"byteLength":bin.len()}],
            "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":png.len()}],
            "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}],
            "images":[{"bufferView":1,"mimeType":"image/png"}],"textures":[{"source":0}],
            "materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0}]}],
            "nodes":[{"mesh":0,"translation":[2,0,0]}],"scenes":[{"nodes":[0]}],"scene":0
        });
        // Folder selection retains relative paths. Geometry needs the BIN, but
        // blueprint rendering must not require external material images.
        let mut external = json.clone();
        external["buffers"][0]["uri"] = serde_json::json!("geometry.bin");
        external["images"] = serde_json::json!([{"uri":"missing-texture.png"}]);
        let folder = AssetFiles::from([
            (
                "selected-folder/guide.gltf".into(),
                serde_json::to_vec(&external).unwrap(),
            ),
            ("selected-folder/geometry.bin".into(), bin.clone()),
        ]);
        let (guides, cache) = crate::blueprints::import(folder.clone(), 20, &init).unwrap();
        assert_eq!(guides.len(), 1);
        let renders = cache[&20].root.get_renders();
        assert_eq!(renders.len(), 1);
        assert_eq!(renders[0].amount, 1);
        assert_eq!(renders[0].model.meshes[0].num_elements, 3);
        let mut incomplete = folder;
        incomplete.remove("selected-folder/geometry.bin");
        let error = crate::blueprints::import(incomplete, 20, &init)
            .err()
            .unwrap();
        let message = format!("{error:#}");
        assert!(message.contains("guide.gltf") && message.contains("geometry.bin"));
        let mut json = serde_json::to_vec(&json).unwrap();
        while json.len() % 4 != 0 {
            json.push(b' ');
        }
        let mut glb = Vec::new();
        glb.extend(0x46546c67u32.to_le_bytes());
        glb.extend(2u32.to_le_bytes());
        glb.extend((28 + json.len() as u32 + bin.len() as u32).to_le_bytes());
        glb.extend((json.len() as u32).to_le_bytes());
        glb.extend(0x4e4f534au32.to_le_bytes());
        glb.extend(json);
        glb.extend((bin.len() as u32).to_le_bytes());
        glb.extend(0x004e4942u32.to_le_bytes());
        glb.extend(bin);
        let files = Arc::new(AssetFiles::from([("guide.glb".into(), glb)]));
        let guide = Blueprint {
            id: 1,
            name: "Guide".into(),
            entry: "guide.glb".into(),
            files: files.clone(),
            transform: InstanceData {
                position: [3., 0., 0.],
                scale: [2.; 3],
                ..Default::default()
            },
            opacity: 0.35,
            visible: true,
        };
        let gpu = crate::blueprints::load(
            &guide,
            &InitContext {
                device: device.clone(),
                queue: queue.clone(),
            },
        )
        .unwrap();
        let children = gpu.root.get_children();
        let imported = &children[0].get_children()[0];
        close(imported.get_world_transform(0).unwrap().position.x, 7.);
        let (restored, _) = project::decode(
            &project::encode(
                &Document {
                    blocks: vec![],
                    blueprints: vec![guide],
                },
                Settings::default(),
            )
            .unwrap(),
        )
        .unwrap();
        let restored = crate::blueprints::load(
            &restored.blueprints[0],
            &InitContext {
                device: device.clone(),
                queue: queue.clone(),
            },
        )
        .unwrap();
        close(
            restored.root.get_children()[0].get_children()[0]
                .get_world_transform(0)
                .unwrap()
                .position
                .x,
            7.,
        );
        assert!(load_model_gltf_from_files(0u32, "missing.gltf", &files, &device, &queue).is_err());
    });
}
