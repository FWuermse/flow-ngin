//! Engine data structures: models, textures, scene graphs, and instances.
//!
//! This module contains the core data types for scene representation:
//!
//! - `model` contains mesh and material definitions, GPU resources for 3D models
//! - `texture` contains GPU texture wrapper and creation utilities
//! - `block` is an instanced building blocks (pre-configured model + instance data)
//! - `instance` holds per-instance transformation and attribute data
//! - `scene_graph` enables hierarchical scene organization
//! - `terrain` will be used for terrain mesh and management

pub mod block;
pub mod instance;
pub mod model;
pub mod scene_graph;
pub mod texture;
pub mod terrain;
