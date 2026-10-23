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
pub mod native_audio;
#[cfg(target_arch = "wasm32")]
mod native_audio_browser;
pub mod native_avatar;
#[cfg(target_arch = "wasm32")]
pub mod native_lot;
pub mod native_terrain;
pub mod snapshot_avatar;
pub mod snapshot_world;
pub mod source_city;
pub mod source_identity;
pub mod source_needs;
pub mod startup;
pub mod world_capture;
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
mod world_flight;
#[cfg(target_arch = "wasm32")]
pub mod world_renderer;

pub mod native_build;
pub mod native_creation;
#[cfg(target_arch = "wasm32")]
mod native_creator_browser;
#[cfg(target_arch = "wasm32")]
mod native_chat_browser;

#[cfg(target_arch = "wasm32")]
pub(crate) mod native_build_browser;
pub mod vm_inbox;
#[cfg(target_arch = "wasm32")]
pub mod world_facade;

#[cfg(target_arch = "wasm32")]
pub mod native_account;
