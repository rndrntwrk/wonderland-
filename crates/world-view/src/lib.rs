#![forbid(unsafe_code)]
//! Validated source world inputs and disposable browser presentation.
//! Rendering a document does not admit a simulation or construction operation.
mod assets;
mod blueprint;
mod document;
mod materials;
mod renderer;
mod scene;
pub use assets::*;
pub use document::*;
pub use renderer::*;
pub use scene::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldError(pub String);
impl std::fmt::Display for WorldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for WorldError {}

impl From<wonderland_render_3d::Error> for WorldError {
    fn from(value: wonderland_render_3d::Error) -> Self {
        Self(value.to_string())
    }
}
impl From<wonderland_render_core::reference::ReferenceError> for WorldError {
    fn from(value: wonderland_render_core::reference::ReferenceError) -> Self {
        Self(value.to_string())
    }
}
