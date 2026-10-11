use wonderland_creator::{Edit, ResourceDocument};
use wonderland_creator_web::session::{browser_limits, EditorSession};
use wonderland_legacy_formats::iff::ChunkKey;

fn fixture() -> Vec<u8> {
    let mut bytes = vec![0; 64];
    let magic = b"IFF FILE 2.5:TYPE FOLLOWED BY SIZE\0 JAMIE DOORNBOS & MAXIS 1";
    bytes[..magic.len()].copy_from_slice(magic);
    let bhav = vec![
        2, 128, 2, 0, 0, 1, 2, 0, 3, 0, 0x12, 0x34, 1, 0, 1, 255, 1, 2, 3, 4, 5, 6, 7, 8, 2, 0,
        254, 255, 8, 7, 6, 5, 4, 3, 2, 1, 0xab,
    ];
    for (kind, id, label, data) in [
        (*b"BHAV", 4096u16, "Init", bhav),
        (*b"BCON", 4096, "Constants", vec![2, 0, 10, 0, 20, 0]),
        (*b"ZZZZ", 9, "Unrecognized resource", vec![0, 1, 0xff, 2, 3]),
    ] {
        bytes.extend_from_slice(&kind);
        bytes.extend_from_slice(&((76 + data.len()) as u32).to_be_bytes());
        bytes.extend_from_slice(&id.to_be_bytes());
        bytes.extend_from_slice(&0u16.to_be_bytes());
        let mut name = [0; 64];
        name[..label.len()].copy_from_slice(label.as_bytes());
        bytes.extend_from_slice(&name);
        bytes.extend_from_slice(&data);
    }
    bytes
}
fn key() -> ChunkKey {
    ChunkKey {
        kind: *b"BHAV",
        id: 4096,
    }
}
fn loaded() -> EditorSession {
    let mut session = EditorSession::default();
    let input = fixture();
    let ticket = session.begin_open(input.len()).unwrap();
    session
        .complete_open(ticket, "workbench.iff", &input)
        .unwrap();
    session
}
fn branch(pointer: u8) -> Edit {
    Edit::BhavBranch {
        instruction: 0,
        true_pointer: pointer,
        false_pointer: 255,
    }
}

#[test]
fn byte_exact_import_filter_and_guarded_export_reopen() {
    let mut s = loaded();
    assert_eq!(s.export().unwrap(), fixture());
    assert!(!s.is_dirty());
    assert_eq!(s.rows("bcon", 0, 100).unwrap()[0].label, "Constants");
    assert_eq!(s.rows("4096", 0, 1).unwrap().len(), 1);
    let before = s.inspect().unwrap();
    assert_eq!(before.guard.format_version, Some(0x8002));
    assert_eq!(
        before.details["instructions"][0]["operand_hex"],
        "0102030405060708"
    );
    assert!(s
        .apply(key(), &before.guard, branch(254), "Change branch")
        .unwrap());
    assert!(s.is_dirty());
    let exported = s.export().unwrap();
    ResourceDocument::import(&exported, &browser_limits())
        .unwrap()
        .validate(&browser_limits())
        .unwrap();
    assert_eq!(s.undo_len(), 1);
    assert!(s.undo().unwrap());
    assert_eq!(s.export().unwrap(), fixture());
    assert!(!s.is_dirty());
    assert!(s.redo().unwrap());
    assert_eq!(s.export().unwrap(), exported);
}

#[test]
fn stale_guard_and_invalid_branch_preserve_bytes_selection_and_history() {
    let mut s = loaded();
    let view = s.inspect().unwrap();
    assert!(s
        .apply(key(), &view.guard, branch(2), "Invalid jump")
        .is_err());
    assert_eq!(s.export().unwrap(), fixture());
    assert_eq!(s.undo_len(), 0);
    s.apply(key(), &view.guard, branch(254), "Valid jump")
        .unwrap();
    let bytes = s.export().unwrap();
    let selected = s.selected();
    assert!(s
        .apply(key(), &view.guard, branch(255), "Stale draft")
        .is_err());
    assert_eq!(s.export().unwrap(), bytes);
    assert_eq!(s.selected(), selected);
    assert_eq!(s.undo_len(), 1);
    assert_eq!(s.redo_len(), 0);
}

#[test]
fn late_or_failed_reads_never_replace_a_newer_document() {
    let mut s = loaded();
    let input = fixture();
    let old = s.begin_open(input.len()).unwrap();
    let new = s.begin_open(input.len()).unwrap();
    s.complete_open(new, "new.iff", &input).unwrap();
    assert!(s.complete_open(old, "late.iff", &input).is_err());
    assert_eq!(s.filename(), "new.iff");
    let bad = s.begin_open(3).unwrap();
    assert!(s.complete_open(bad, "bad.iff", b"bad").is_err());
    assert_eq!(s.filename(), "new.iff");
    assert_eq!(s.export().unwrap(), input);
    assert!(s.begin_open(browser_limits().max_input_bytes + 1).is_err());
}

#[test]
fn bounded_undo_evicts_oldest_and_new_edit_discards_redo() {
    let input = fixture();
    let mut s = EditorSession::new(browser_limits(), input.len() * 2).unwrap();
    let t = s.begin_open(input.len()).unwrap();
    s.complete_open(t, "history.iff", &input).unwrap();
    for pointer in [254, 255, 1] {
        let guard = s.inspect().unwrap().guard;
        s.apply(key(), &guard, branch(pointer), "Branch").unwrap();
    }
    assert_eq!(s.undo_len(), 2);
    assert!(s.history_bytes() <= input.len() * 2);
    s.undo().unwrap();
    assert_eq!(s.redo_len(), 1);
    let guard = s.inspect().unwrap().guard;
    s.apply(key(), &guard, branch(254), "Replacement").unwrap();
    assert_eq!(s.redo_len(), 0);
    assert!(s.history_bytes() <= input.len() * 2);
}

#[test]
fn insufficient_history_budget_rejects_before_mutation_and_noop_does_not_grow_history() {
    let input = fixture();
    let mut s = EditorSession::new(browser_limits(), input.len() - 1).unwrap();
    let t = s.begin_open(input.len()).unwrap();
    s.complete_open(t, "tiny.iff", &input).unwrap();
    let guard = s.inspect().unwrap().guard;
    assert!(s.apply(key(), &guard, branch(254), "Too large").is_err());
    assert_eq!(s.export().unwrap(), input);
    assert_eq!(s.history_bytes(), 0);
    let mut s = loaded();
    let guard = s.inspect().unwrap().guard;
    assert!(!s.apply(key(), &guard, branch(1), "No change").unwrap());
    assert_eq!(s.undo_len(), 0);
    assert!(!s.is_dirty());
}

#[test]
fn unknown_bytes_are_visible_and_editable_without_changing_other_resources() {
    let mut s = loaded();
    let unknown = ChunkKey {
        kind: *b"ZZZZ",
        id: 9,
    };
    s.select(unknown).unwrap();
    let inspection = s.inspect().unwrap();
    assert_eq!(inspection.details["preview_hex"], "0001ff0203");
    s.apply(
        unknown,
        &inspection.guard,
        Edit::UnknownBytes(vec![7, 8]),
        "Replace unknown bytes",
    )
    .unwrap();
    let doc = ResourceDocument::import(&s.export().unwrap(), &browser_limits()).unwrap();
    assert_eq!(doc.chunk(unknown).unwrap().data, vec![7, 8]);
    let original = ResourceDocument::import(&fixture(), &browser_limits()).unwrap();
    assert_eq!(
        doc.chunk(key()).unwrap().data,
        original.chunk(key()).unwrap().data
    );
}

#[test]
fn growing_resource_keeps_latest_edit_undoable_with_a_full_history() {
    let input = fixture();
    let mut s = EditorSession::new(browser_limits(), input.len() * 2).unwrap();
    let ticket = s.begin_open(input.len()).unwrap();
    s.complete_open(ticket, "growth.iff", &input).unwrap();
    for pointer in [254, 255] {
        let g = s.inspect().unwrap().guard;
        s.apply(key(), &g, branch(pointer), "Branch").unwrap();
    }
    let before = s.export().unwrap();
    let unknown = ChunkKey {
        kind: *b"ZZZZ",
        id: 9,
    };
    s.select(unknown).unwrap();
    let guard = s.inspect().unwrap().guard;
    s.apply(
        unknown,
        &guard,
        Edit::UnknownBytes(vec![7; 64]),
        "Grow resource",
    )
    .unwrap();
    let grown = s.export().unwrap();
    assert!(grown.len() > before.len());
    assert!(s.undo().unwrap());
    assert_eq!(s.export().unwrap(), before);
    assert!(s.redo().unwrap());
    assert_eq!(s.export().unwrap(), grown);
    assert!(s.history_bytes() <= input.len() * 2);
}

#[test]
fn unretainable_expansion_is_rejected_without_publishing_candidate() {
    let input = fixture();
    let mut s = EditorSession::new(browser_limits(), input.len()).unwrap();
    let ticket = s.begin_open(input.len()).unwrap();
    s.complete_open(ticket, "expansion.iff", &input).unwrap();
    let unknown = ChunkKey {
        kind: *b"ZZZZ",
        id: 9,
    };
    s.select(unknown).unwrap();
    let guard = s.inspect().unwrap().guard;
    assert!(s
        .apply(
            unknown,
            &guard,
            Edit::UnknownBytes(vec![7; 64]),
            "Too much history"
        )
        .is_err());
    assert_eq!(s.export().unwrap(), input);
    assert_eq!(s.undo_len(), 0);
    assert_eq!(s.selected(), Some(unknown));
}

#[test]
fn editing_supersedes_a_pending_import_without_losing_the_edit() {
    let mut s = loaded();
    let input = fixture();
    let ticket = s.begin_open(input.len()).unwrap();
    let guard = s.inspect().unwrap().guard;
    s.apply(key(), &guard, branch(254), "Edit while reading")
        .unwrap();
    let changed = s.export().unwrap();
    assert!(!s.is_current_read(ticket));
    assert!(s.complete_open(ticket, "late.iff", &input).is_err());
    assert_eq!(s.export().unwrap(), changed);
    assert_eq!(s.undo_len(), 1);
    let ticket = s.begin_open(input.len()).unwrap();
    s.undo().unwrap();
    assert!(!s.is_current_read(ticket));
    assert_eq!(s.export().unwrap(), input);
}

fn sprite_fixture(version: u32, invalid: bool) -> Vec<u8> {
    let mut bytes = fixture()[..64].to_vec();
    let mut palette = vec![1, 0, 0, 0, 3, 0, 0, 0];
    palette.extend([0; 8]);
    palette.extend([11, 12, 13, 21, 22, 23, 31, 32, 33]);
    let frame = vec![
        4, 0, 2, 0, 7, 0, 0, 0, 0, 0, 0, 0, 254, 255, 5, 0, 18, 0, 1, 192, 1, 231, 1, 64, 19, 2,
        15, 210, 1, 32, 9, 1, 1, 96, 1, 128, 0, 160, 0, 0,
    ];
    let mut sprite = version.to_le_bytes().to_vec();
    if version == 1000 {
        for value in [1u32, 7, 16] {
            sprite.extend(value.to_le_bytes());
        }
    } else {
        for value in [7u32, 1, 1001, frame.len() as u32] {
            sprite.extend(value.to_le_bytes());
        }
    }
    sprite.extend(frame);
    if invalid {
        sprite = vec![0];
    }
    for (kind, id, data) in [(*b"PALT", 7u16, palette), (*b"SPR2", 8, sprite)] {
        bytes.extend(kind);
        bytes.extend(((76 + data.len()) as u32).to_be_bytes());
        bytes.extend(id.to_be_bytes());
        bytes.extend([0; 66]);
        bytes.extend(data);
    }
    bytes
}

#[test]
fn malformed_sprite_is_raw_inspectable_and_never_claimed_authorable() {
    let bytes = sprite_fixture(1001, true);
    let mut s = EditorSession::default();
    let ticket = s.begin_open(bytes.len()).unwrap();
    s.complete_open(ticket, "invalid-sprite.iff", &bytes)
        .unwrap();
    s.select(ChunkKey {
        kind: *b"SPR2",
        id: 8,
    })
    .unwrap();
    let view = s.inspect().unwrap();
    assert!(view.error.is_some());
    assert_eq!(view.details["preview_hex"], "00");
    assert!(view.details.get("sprite_package").is_none());
    assert_eq!(view.format_version, None);
    assert_eq!(s.export().unwrap(), bytes);
}

#[test]
fn sprite_inspection_validates_real_planes_and_dependencies_before_showing_version() {
    for version in [1000, 1001] {
        let bytes = sprite_fixture(version, false);
        let mut s = EditorSession::default();
        let ticket = s.begin_open(bytes.len()).unwrap();
        s.complete_open(ticket, "valid-sprite.iff", &bytes).unwrap();
        s.select(ChunkKey {
            kind: *b"SPR2",
            id: 8,
        })
        .unwrap();
        let view = s.inspect().unwrap();
        assert_eq!(view.error, None);
        assert_eq!(view.format_version, Some(version));
        assert_eq!(view.details["sprite_package"], true);
        // Existing ResourceDocument guards have no SPR2 version discriminator;
        // truthful display metadata must not manufacture a mismatching guard.
        assert_eq!(view.guard.format_version, None);
        assert_eq!(s.export().unwrap(), bytes);
    }
}

#[test]
fn oversized_typed_preview_falls_back_to_bounded_raw_bytes_without_losing_source() {
    let mut payload = vec![255, 255, 4, 0];
    for _ in 0..4 {
        payload.extend(vec![1; 60_000]);
        payload.push(0);
    }
    let mut input = fixture()[..64].to_vec();
    input.extend(*b"STR#");
    input.extend(((76 + payload.len()) as u32).to_be_bytes());
    input.extend(4u16.to_be_bytes());
    input.extend([0; 66]);
    input.extend(payload);
    let mut s = EditorSession::default();
    let ticket = s.begin_open(input.len()).unwrap();
    s.complete_open(ticket, "large-text.iff", &input).unwrap();
    let view = s.inspect().unwrap();
    assert!(view.error.unwrap().contains("preview exceeds"));
    assert!(view.details.get("sets").is_none());
    assert_eq!(view.details["preview_hex"].as_str().unwrap().len(), 4096);
    assert_eq!(s.export().unwrap(), input);
}

#[test]
fn browser_sprite_package_cap_is_enforced_before_parsing_or_mutating() {
    let mut s = loaded();
    let original = s.export().unwrap();
    let oversized = vec![b' '; wonderland_creator_web::session::MAX_SPRITE_JSON_BYTES + 1];
    assert!(s
        .apply_sprite_json(&oversized)
        .unwrap_err()
        .contains("1 MiB"));
    assert_eq!(s.export().unwrap(), original);
    assert_eq!(s.undo_len(), 0);
}

#[test]
fn unknown_source_language_entries_are_visible_and_preserved() {
    let mut payload = vec![253, 255, 1, 0, 21];
    payload.extend(b"Unassigned text\0Source comment\0");
    let mut input = fixture()[..64].to_vec();
    input.extend(*b"STR#");
    input.extend(((76 + payload.len()) as u32).to_be_bytes());
    input.extend(21u16.to_be_bytes());
    input.extend([0; 66]);
    input.extend(payload);
    let mut s = EditorSession::default();
    let ticket = s.begin_open(input.len()).unwrap();
    s.complete_open(ticket, "languages.iff", &input).unwrap();
    let view = s.inspect().unwrap();
    assert_eq!(view.error, None);
    assert_eq!(view.details["string_format"], -3);
    assert_eq!(view.details["unassigned"][0]["language"], 21);
    assert_eq!(view.details["unassigned"][0]["value"], "Unassigned text");
    assert_eq!(view.details["unassigned"][0]["comment"], "Source comment");
    assert_eq!(s.export().unwrap(), input);
}
