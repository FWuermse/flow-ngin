# Flow NGIN

A simple cross-plattform instancing-oriented game engine with focus on full WASM-compatibility.

## Features

- [ ] Model loading:
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

## Supported backends

+ Vulkan
+ Metal
+ DX12
+ WebGL (incl. WASM)
+ WebGPU (incl. WASM)
