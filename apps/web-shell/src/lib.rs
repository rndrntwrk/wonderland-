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
