//! Browser presentation adapter; shared game contracts remain DOM free.
pub mod feedback;
pub mod fixture;
pub mod geometry;

#[cfg(target_arch = "wasm32")]
pub mod app;
#[cfg(target_arch = "wasm32")]
pub mod bridge;
#[cfg(target_arch = "wasm32")]
pub mod components;
#[cfg(target_arch = "wasm32")]
pub mod screens;

pub mod authoring_adapter;
#[cfg(target_arch = "wasm32")]
pub mod authoring_bridge;
pub mod authoring_geometry;
pub mod persistence;

#[cfg(target_arch = "wasm32")]
pub mod avatar_content;
#[cfg(target_arch = "wasm32")]
pub mod avatar_renderer;
pub mod source_identity;
