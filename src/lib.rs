//! flow-ngin
//!
//! A lightweight, cross-platform instancing-oriented game engine focused on
//! native and WASM compatibility. This crate exposes a small surface for
//! constructing GPU resources, rendering pipelines and scene data. The design
//! emphasizes reuse of pipelines, efficient instancing and a minimal runtime
//! surface suitable for embedding in native applications or the web.
//!
//! High-level modules
//! - `camera`: camera types, controller and uniforms for view/projection
//! - `context`: central GPU and window context that owns device/queue/pipelines
//! - `data_structures`: engine data models (meshes, instances, textures)
//! - `flow`: high level flow control (scenes / update loops)
//! - `pick`: object picking utilities and shaders
//! - `pipelines`: definitions for various render pipelines (basic, light, gui)
//! - `resources`: helpers to load textures/models and create GPU resources
//! - `render`: render composition for efficient pipeline reuse
//!

pub mod camera;
pub mod context;
pub mod data_structures;
pub mod flow;
pub mod pick;
pub mod pipelines;
pub mod resources;
pub mod render;

// Re-exports commonly used types for convenience in downstream code.
pub use winit::dpi::PhysicalPosition;
pub use cgmath::*;
pub use winit::event::DeviceEvent;
pub use winit::event::WindowEvent;
pub use wgpu::*;
