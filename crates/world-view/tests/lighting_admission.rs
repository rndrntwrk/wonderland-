use wonderland_world_view::WorldDocument;

#[test]
fn malformed_lighting_must_not_be_silently_admitted() {
    let world = WorldDocument::original_empty_lot().unwrap();
    let mut json = serde_json::to_value(&world).unwrap();
    json["lighting"] = serde_json::json!({
        "source": vec![9; 32], "lot_id": "7", "epoch": "3", "revision": "1",
        "width": 0, "height": 0, "stories": 1,
        "cells": [], "rooms": [], "geometry": [], "lights": [],
        "minimum": [0,0,0,0], "outside": [255,255,255,255]
    });
    let result = serde_json::from_value::<WorldDocument>(json)
        .map_err(|e| e.to_string())
        .and_then(|document| document.validate().map_err(|e| e.to_string()));
    assert!(
        result.is_err(),
        "malformed lighting was silently accepted as an unlit document"
    );
}
