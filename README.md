# Flow NGIN

A simple cross-plattform instancing-oriented game engine with focus on full WASM-compatibility.

You may want to use this engine if:
- You heavily rely on instancing (many instances of the same model)
- You need full browser compatibility
- You like to use Rust exclusively for all platforms
- You don't need a GUI for game development
- You prefer code over low-code

## Features

- [x] Model loading:
  - [x] Loading OBJ files
    - [x] Meshes
    - [x] Normals
    - [x] Tex-coords
  - [x] Loading GLTF files
    - [x] Meshes
    - [x] Normals
    - [x] Tex-coords
    - [x] Tangents
    - [x] Bitangents
    - [x] Textures
    - [x] Normal Maps
    - [x] Multiple Animation Tracks
    - [ ] Rigs (Not planned at the moment)
- [x] Light
- [x] Animations
  - [x] Hierarchies
  - [x] Position Interpolation
  - [x] Time-based
- [x] Camera
- [ ] Audio
- [x] Shading
  - [x] Blinn-Phong
  - [x] Normal Map support
- [ ] Shadows
- [x] Picking 
- [x] Ray casting
- [ ] Terrain generation
  - [ ] Multi-texture Terrain
  - [ ] Deterministic Terrain generation
  - [ ] Seed as input parameter
- [ ] User Interface
  - [ ] Button
  - [ ] Icons (including transparency)
  - [ ] Responsiveness

## Running Integration Tests

Note: integrations tests with golden-image-tests can currently only be executed on Wayland and Windows.

```sh
cargo test --features integration-tests
```

## Supported Backends

+ Vulkan
+ Metal
+ DX12
+ WebGL (incl. WASM)
+ WebGPU (incl. WASM)
