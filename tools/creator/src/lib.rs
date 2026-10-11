// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, obtain one at https://mozilla.org/MPL/2.0/.
#![forbid(unsafe_code)]
mod hash;
mod workspace;
pub use hash::sha256;
pub use workspace::Workspace;
pub mod city;
pub mod editors;
pub mod json_support;
pub mod patch_view;
mod resources;
pub use resources::{
    Edit, EditGuard, ResourceDocument, ResourceGuard, ResourceOperation, ResourceTransaction,
};
mod transaction;
pub use transaction::{decode_hex, MAX_TRANSACTION_SPEC_BYTES};
pub mod debug;
pub use resources::{validate_cfg, CfgReport};
pub fn default_limits() -> wonderland_legacy_formats::Limits {
    wonderland_legacy_formats::Limits {
        max_input_bytes: 64 * 1024 * 1024,
        max_resource_bytes: 16 * 1024 * 1024,
        max_total_decoded_bytes: 192 * 1024 * 1024,
        max_entries: 10_000,
        max_string_bytes: 1024 * 1024,
        max_pixels: 1024 * 1024,
        ..Default::default()
    }
}
pub const TOOL_INVENTORY: &str = include_str!("../../../docs/compat/tools.json");

mod metadata;
pub use metadata::{hex, json_string};

mod tuning;
pub use tuning::TuningDocument;
