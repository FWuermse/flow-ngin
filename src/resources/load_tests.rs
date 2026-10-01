use super::*;
use crate::context::InitContext;

#[test]
fn extensions_ignore_url_suffixes_and_case() {
    for (path, expected) in [
        ("ship.OBJ", "obj"),
        ("https://example.com/ship.GLB?v=.obj#scene", "glb"),
        ("ship.gltf#preview", "gltf"),
        ("no-extension", ""),
        ("https://example.obj?download", ""),
        ("//example.gltf/asset", ""),
        ("data.bin", "bin"),
    ] {
        assert_eq!(asset_extension(path), expected);
    }
}

#[tokio::test]
async fn loads_owned_assets_and_preserves_errors() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        flags: wgpu::InstanceFlags::default(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        backend_options: wgpu::BackendOptions::default(),
        display: None,
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .expect("GPU adapter required");
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .unwrap();
    let gpu = || InitContext {
        device: device.clone(),
        queue: queue.clone(),
    };
    let path = "cube.obj";
    let (returned, asset) = load_asset(path.into(), PickId(1), gpu()).await.unwrap();
    assert_eq!(returned, path);
    assert!(matches!(asset, Asset::Model(_)));

    let path = "metal.gltf";
    let (first, second) = tokio::join!(
        load_asset(path.into(), PickId(41), gpu()),
        load_asset(path.into(), PickId(42), gpu()),
    );
    for (result, id) in [(first, PickId(41)), (second, PickId(42))] {
        let (returned, Asset::Scene(scene)) = result.unwrap() else {
            panic!("expected scene")
        };
        assert_eq!(returned, path);
        let renders = scene.get_renders();
        assert!(!renders.is_empty());
        assert!(renders.iter().all(|render| render.id == id));
    }

    let path = "metal.bin";
    let (first, second) = tokio::join!(
        load_asset(path.into(), PickId(1), gpu()),
        load_asset(path.into(), PickId(2), gpu()),
    );
    let (returned, Asset::Bytes(mut first)) = first.unwrap() else {
        panic!("expected bytes")
    };
    let (_, Asset::Bytes(second)) = second.unwrap() else {
        panic!("expected bytes")
    };
    assert_eq!(returned, path);
    assert_eq!(first, second);
    first[0] ^= 1;
    assert_ne!(first, second);

    let dir = std::env::temp_dir().join(format!("flow-load-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let glb_path = dir.join("scene.GLB").to_str().unwrap().to_owned();
    let mut json = std::fs::read("assets/metal.gltf").unwrap();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    let mut glb = b"glTF".to_vec();
    glb.extend(2u32.to_le_bytes());
    glb.extend(((20 + json.len()) as u32).to_le_bytes());
    glb.extend((json.len() as u32).to_le_bytes());
    glb.extend(b"JSON");
    glb.extend(json);
    std::fs::write(&glb_path, glb).unwrap();
    let (returned, Asset::Scene(scene)) = load_asset(glb_path.clone(), PickId(43), gpu())
        .await
        .unwrap()
    else {
        panic!("expected GLB scene")
    };
    assert_eq!(returned, glb_path);
    assert!(
        scene
            .get_renders()
            .iter()
            .all(|render| render.id == PickId(43))
    );

    let missing = dir.join("retry.bin").to_str().unwrap().to_owned();
    let Err(error) = load_asset(missing.clone(), PickId(3), gpu()).await else {
        panic!("expected error")
    };
    assert_eq!(error.path, missing);
    assert_eq!(
        error
            .source
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .kind(),
        std::io::ErrorKind::NotFound
    );
    std::fs::write(&missing, b"retry").unwrap();
    let (returned, Asset::Bytes(bytes)) =
        load_asset(missing.clone(), PickId(3), gpu()).await.unwrap()
    else {
        panic!("expected bytes")
    };
    assert_eq!(returned, missing);
    assert_eq!(bytes, b"retry");

    let broken = dir.join("broken.glb").to_str().unwrap().to_owned();
    std::fs::write(&broken, b"invalid glb").unwrap();
    let Err(error) = load_asset(broken.clone(), PickId(4), gpu()).await else {
        panic!("expected decode error")
    };
    assert_eq!(error.path, broken);

    let obj = dir
        .join("missing-material.obj")
        .to_str()
        .unwrap()
        .to_owned();
    std::fs::write(
        &obj,
        "mtllib missing-flow-load-test.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
    )
    .unwrap();
    let Err(error) = load_asset(obj.clone(), PickId(5), gpu()).await else {
        panic!("expected material error")
    };
    assert_eq!(error.path, obj);
    assert!(matches!(
        error.source.downcast_ref::<tobj::LoadError>(),
        Some(tobj::LoadError::OpenFileFailed)
    ));
    let assets = load_assets(vec![
        ("cube.obj".into(), PickId(1)),
        (glb_path.clone(), PickId(51)),
        ("metal.gltf".into(), PickId(52)),
        ("metal.gltf".into(), PickId(53)),
        ("metal.bin".into(), PickId(2)),
        ("metal.bin".into(), PickId(3)),
    ], gpu()).await.unwrap();
    assert_eq!(assets.iter().map(|(path, _)| path.as_str()).collect::<Vec<_>>(),
        ["cube.obj", glb_path.as_str(), "metal.gltf", "metal.gltf", "metal.bin", "metal.bin"]);
    let mut byte_assets = Vec::new();
    for (index, (_, asset)) in assets.into_iter().enumerate() {
        match asset {
            Asset::Model(_) => assert_eq!(index, 0),
            Asset::Scene(scene) => {
                let renders = scene.get_renders();
                assert!(!renders.is_empty());
                assert!(renders.iter().all(|render| render.id == PickId(50 + index as u32)));
            }
            Asset::Bytes(bytes) => byte_assets.push(bytes),
        }
    }
    assert_eq!(byte_assets.len(), 2);
    assert_eq!(byte_assets[0], byte_assets[1]);
    byte_assets[0][0] ^= 1;
    assert_ne!(byte_assets[0], byte_assets[1]);

    let absent = dir.join("absent.bin").to_str().unwrap().to_owned();
    let Err(errors) = load_assets(vec![
        (absent.clone(), PickId(1)),
        ("cube.obj".into(), PickId(2)),
        (broken.clone(), PickId(3)),
        (absent.clone(), PickId(4)),
    ], gpu()).await else { panic!("expected all errors") };
    assert_eq!(errors.iter().map(|error| error.path.as_str()).collect::<Vec<_>>(),
        [absent.as_str(), broken.as_str(), absent.as_str()]);
    assert!(errors[0].source.downcast_ref::<std::io::Error>().is_some());
    assert!(load_assets(vec![], gpu()).await.unwrap().is_empty());

    std::fs::remove_dir_all(dir).unwrap();
}
