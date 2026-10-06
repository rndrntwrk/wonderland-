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

pub mod audio_bridge;
#[cfg(target_arch = "wasm32")]
pub mod avatar_content;
#[cfg(target_arch = "wasm32")]
pub mod avatar_renderer;
pub mod connected_adapter;
pub mod live_world_adapter;
pub mod snapshot_avatar;
pub mod snapshot_world;
pub mod source_city;
pub mod source_identity;
pub mod source_needs;
pub mod startup;
pub mod vm_delivery;
pub mod world_draft;

#[cfg(target_arch = "wasm32")]
pub mod connected;
#[cfg(target_arch = "wasm32")]
pub mod connected_authoring;
#[cfg(target_arch = "wasm32")]
pub mod connected_bridge;
#[cfg(target_arch = "wasm32")]
pub mod connected_world;
#[cfg(target_arch = "wasm32")]
pub mod source_city_renderer;
#[cfg(target_arch = "wasm32")]
pub mod source_world_screen;
#[cfg(target_arch = "wasm32")]
pub mod startup_view;
#[cfg(target_arch = "wasm32")]
pub mod world_renderer;
