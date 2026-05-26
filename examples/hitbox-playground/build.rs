use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root_assets = manifest_dir.join("../../assets");
    let local_assets = manifest_dir.join("assets");

    // Tell Cargo to rerun this script when root assets change
    println!("cargo:rerun-if-changed=../../assets");

    // Files needed from the root assets directory
    let needed = &[
        // Shared 3D model assets
        "cube.obj",
        "cube.mtl",
        "cube-diffuse.jpg",
        "cube-normal.png",
        // UI atlas
        "card_atlas.png",
        // Playground-specific models (created in root assets)
        "plane.obj",
        "plane.mtl",
        "overlay-white.obj",
        "overlay-white.mtl",
        "overlay-yellow.obj",
        "overlay-yellow.mtl",
        "overlay-red.obj",
        "overlay-red.mtl",
        // Solid-color textures for overlays
        "white-diffuse.png",
        "yellow-diffuse.png",
        "red-diffuse.png",
        "flat-normal.png",
    ];

    fs::create_dir_all(&local_assets).expect("Failed to create local assets dir");
    fs::create_dir_all(local_assets.join("fonts")).expect("Failed to create fonts dir");

    for file in needed {
        let src = root_assets.join(file);
        let dst = local_assets.join(file);
        if src.exists() {
            fs::copy(&src, &dst)
                .unwrap_or_else(|e| panic!("Failed to copy {file}: {e}"));
        } else {
            eprintln!("cargo:warning=Asset not found: {}", src.display());
        }
    }
}
