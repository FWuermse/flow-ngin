# Schematic editor

Run the native editor from the repository root:

```sh
cargo run --manifest-path examples/schematic-editor/Cargo.toml
```

Block models, materials, textures, and the 8 × 8 icon atlas download from
`https://play.fortressflow.com/assets/` at startup. The editor remains usable
while they load. Failed downloads retain the cube or plain-button fallback and
report an error in the status bar. Internet access is required for these assets.

Edit `src/assets.rs` to change the base URL, per-block OBJ paths, or icon slots.
Atlas slots are zero-based and row-major. Gold has no model URL yet and uses the
fallback cube; copper, iron, and gold use icon slot 38. Build buttons show their
resource icon without labels: tile 0 is inactive, tile 3 is hovered, and tile 1
is selected. Checkboxes use tile 0 with tile
41 overlaid when checked.

The supplied OBJ and MTL references resolve relative to the server's `/assets/`
directory. The downloader bundles their dependencies for the in-memory loader.
Exports with one material and no `usemtl` assignment (such as the log) are
assigned that material so both diffuse and normal maps are used.
Model dimensions are preserved, with the bottom moved to y=0 for placement.
Re-enabling grid snap resets active preview rotations and snaps its placement;
placed blocks remain unchanged. Grid lines follow block edges at half-unit X/Z
coordinates. Selection, snapping, and collision continue to use the editor's logical block
boxes, rather than the detailed rendered shapes.

The fallback cube is embedded from the repository's existing `assets/cube.obj`,
`cube.mtl`, `cube-diffuse.jpg`, and `cube-normal.png` at build time.

The WASM build also supports these downloads. Cross-origin server configuration
is separate from this example; native execution does not require browser CORS.

Run tests:

```sh
cargo test --manifest-path examples/schematic-editor/Cargo.toml --lib
# Optional: fetch every hosted asset and validate GPU model/atlas creation.
cargo test --manifest-path examples/schematic-editor/Cargo.toml hosted_assets_load_on_gpu -- --ignored --nocapture
```

Blueprint imports render geometry as a tinted guide. A standalone OBJ does not
need MTL or texture files. For glTF, select the containing folder (or the glTF
and its referenced BIN files); material images are optional. Import failures
appear first in the status area and are not overwritten by background asset
downloads.
