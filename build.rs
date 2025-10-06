use anyhow::*;
use fs_extra::copy_items;
use fs_extra::dir::CopyOptions;
use std::env;
use std::path::PathBuf;

fn main() -> Result<()> {
    // This tells Cargo to rerun this script if something in /assets/ changes.
    println!("cargo:rerun-if-changed=assets/*");

    let out_dir = env::var("OUT_DIR")?;
    let mut copy_options = CopyOptions::new();
    copy_options.overwrite = true;
    let mut paths_to_copy = Vec::new();
    paths_to_copy.push("assets/");
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let assets_src = manifest_dir.join("assets");
    if assets_src.exists() {
        copy_items(&paths_to_copy, out_dir, &copy_options)?;
    }

    Ok(())
}
