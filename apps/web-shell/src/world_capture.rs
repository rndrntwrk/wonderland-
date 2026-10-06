//! Capture metadata and receipt validation; no GPU handles or simulation writes.
use serde::Deserialize;
use wonderland_world_view::{ViewportControls, WorldDocument, WorldRenderStats};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorldCaptureReceipt {
    pub generation: String,
    pub width: u32,
    pub height: u32,
    pub image_url: String,
    pub metadata_url: String,
    pub filename: String,
    pub metadata_filename: String,
}
impl WorldCaptureReceipt {
    pub fn matches(&self, generation: &str, width: u32, height: u32) -> bool {
        self.generation == generation
            && self.width == width
            && self.height == height
            && width > 0
            && height > 0
            && width <= 4096
            && height <= 4096
            && u64::from(width) * u64::from(height) <= 1_048_576
            && self.filename == format!("wonderland-source-view-{generation}.png")
            && self.metadata_filename == format!("wonderland-source-view-{generation}.json")
            && [&self.image_url, &self.metadata_url].iter().all(|value| {
                value.starts_with("blob:")
                    && value.len() <= 2048
                    && !value.chars().any(char::is_control)
            })
    }
}

pub fn capture_metadata(
    world: &WorldDocument,
    controls: ViewportControls,
    stats: &WorldRenderStats,
) -> Result<String, String> {
    let diagnostics: Vec<_> = stats
        .diagnostics
        .iter()
        .take(64)
        .map(|item| item.message.chars().take(512).collect::<String>())
        .collect();
    let encoded = serde_json::to_string(&serde_json::json!({
        "provenance": world.provenance,
        "revision": world.revision,
        "view": controls,
        "lot": {"width": world.lot.width, "height": world.lot.height, "levels": world.lot.levels},
        "rendered": {"parts": stats.parts, "triangles": stats.triangles},
        "diagnostics": diagnostics,
        "diagnostics_total": stats.diagnostics.len(),
        "diagnostics_truncated": stats.diagnostics.len() > 64 || stats.diagnostics.iter().take(64).any(|item| item.message.chars().count() > 512),
    })).map_err(|error| error.to_string())?;
    if encoded.len() > 65_536 {
        return Err("Capture metadata exceeds this browser's budget.".into());
    }
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receipt_requires_exact_frame_dimensions_urls_and_filenames() {
        let generation = "18446744073709551615";
        let receipt = WorldCaptureReceipt {
            generation: generation.into(),
            width: 640,
            height: 480,
            image_url: "blob:https://localhost/image".into(),
            metadata_url: "blob:https://localhost/details".into(),
            filename: format!("wonderland-source-view-{generation}.png"),
            metadata_filename: format!("wonderland-source-view-{generation}.json"),
        };
        assert!(receipt.matches(generation, 640, 480));
        assert!(!receipt.matches("1", 640, 480));
        assert!(!receipt.matches(generation, 320, 240));
        let mut invalid = receipt.clone();
        invalid.image_url = "javascript:alert(1)".into();
        assert!(!invalid.matches(generation, 640, 480));
        invalid = receipt.clone();
        invalid.metadata_url = "https://external.test/details".into();
        assert!(!invalid.matches(generation, 640, 480));
        invalid = receipt;
        invalid.filename = "../world.fsov".into();
        assert!(!invalid.matches(generation, 640, 480));
    }
    #[test]
    fn metadata_preserves_source_revisions_and_camera_without_mutating_world() {
        let mut world = WorldDocument::original_empty_lot().unwrap();
        world.revision.tick = 9_007_199_254_740_993;
        let original = world.clone();
        let controls = ViewportControls::default();
        let value: serde_json::Value = serde_json::from_str(
            &capture_metadata(&world, controls, &WorldRenderStats::default()).unwrap(),
        )
        .unwrap();
        assert_eq!(value["revision"]["tick"], "9007199254740993");
        assert_eq!(value["view"]["zoom"], 1.);
        assert_eq!(value["provenance"]["origin"], world.provenance.origin);
        assert_eq!(world, original);
    }
}
