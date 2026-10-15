//! Typed boundary harnesses, not original content qualification.
use super::*;
use wonderland_game_runtime::{
    GameRuntime, LotModel, RuntimeConfig, RuntimeRole, VmMode,
    live_wire::{CheckpointRequest, encode_checkpoint_request},
    sim_core::{avatars::state::TEMPLATE_PERSON, lot_permissions::LotPermissionState},
};
#[path = "../examples/support/owned_action_content.rs"]
mod action_content;

fn host() -> Host {
    let content = action_content::content();
    let config = RuntimeConfig::new(VmMode::Tso, 41, 7, 123);
    let mut runtime = GameRuntime::new(
        content,
        LotModel::new(8, 8, 1).unwrap(),
        config.clone(),
        RuntimeRole::Authority,
    )
    .unwrap();
    let source = SourceJoinMetadata {
        source_import_sha256: [1; 32],
        content: runtime.sim().state().content,
        neighborhood_id: 8,
        lot_location: 16777472,
        avatar_guids: [TEMPLATE_PERSON].into(),
        april_fools_2019: false,
        object_flags: BTreeMap::new(),
        registered_groups: BTreeMap::new(),
    };
    runtime
        .advance(vec![
            AcceptedCommand::InitializeLotPermissions {
                lot: LotPermissionState::default(),
            },
            AcceptedCommand::InitializeOwnedAdmission { source },
        ])
        .unwrap();
    Host::new(
        runtime,
        config,
        HostBindingV1 {
            protocol: 1,
            source_pin: "test harness".into(),
            binding_sha256: "a".repeat(64),
            lot_incarnation: DecimalId(1),
        },
        Value::Null,
        None,
    )
    .unwrap()
}
fn export(host: &Host, id: u32) -> ReservedAvatarExportV2 {
    let portable = json!({"name":format!("Owned {id}"),"persistent_id":id.to_string(),"default_suits":{"daywear":"12","swimwear":"13","sleepwear":"14"},"budget_mirror":"123","body":"12","head":"15","skin_tone":0,"is_worker":false,"custom_guid":0,"motive_data":vec![100;16],"person_data":vec![0;27],"relationships":[],"job_info":[],"ignored_avatars":[]});
    let portable_json = serde_json::to_string(&portable).unwrap();
    let mut value = json!({"schema_version":2,"authority_id":"b".repeat(64),"account_id":(1000+id).to_string(),"avatar_id":id.to_string(),"lot_id":"41","control_generation":"1","admission_operation":format!("admission-{id:016}"),"reservation_digest":"d".repeat(64),"session_ref":format!("{id:064x}"),"account_revision":"1","content_release":host.binding.binding_sha256,"protocol_release":"c".repeat(64),"expires_at":60000,"permission_binding":permission(&host.committed).unwrap(),"permission_source":{"registered_at_ms":0,"created_at_ms":0,"observed_at_ms":0,"roommate_lots":[],"moderation_level":0,"mayor_nhood":null},"source_revisions":{"owned_assets":1,"wardrobe":"1".repeat(64),"ledger":"2".repeat(64),"progression":"3".repeat(64),"permissions":"4".repeat(64)},"portable_sha256":Digest::of(portable_json.as_bytes()).as_str(),"portable_json":portable_json});
    let hash = Digest::of(&serde_json::to_vec(&value).unwrap());
    value["export_digest"] = json!(hash.as_str());
    serde_json::from_value(value).unwrap()
}
fn prepare(host: &Host, id: u32, batch: u64) -> RequestV1 {
    RequestV1::PrepareOwnedAdmission {
        batch_id: DecimalId(batch),
        binding: host.binding.clone(),
        predecessor: crate::runtime::cursor(&host.committed).unwrap(),
        expected_control_revision: DecimalId(batch),
        principal: DecimalId(u64::from(id)),
        export: Box::new(export(host, id)),
    }
}
fn commit(host: &mut Host, proposal: &Value, batch: u64) -> Value {
    host.handle(RequestV1::Resolve {
        batch_id: DecimalId(batch),
        digest: proposal["digest"].as_str().unwrap().into(),
        decision: DecisionV1::Committed {
            cursor: serde_json::from_value(proposal["payload"]["result"].clone()).unwrap(),
        },
    })
    .unwrap()
}
#[test]
fn typed_admission_is_commit_only_idempotent_and_checkpoint_restorable() {
    let mut host = host();
    let initial = checkpoint(&host.committed).unwrap();
    let request = prepare(&host, 17, 1);
    let repeated = prepare(&host, 17, 1);
    let proposal = host.handle(request).unwrap();
    assert_eq!(checkpoint(&host.committed).unwrap(), initial);
    assert_eq!(host.handle(repeated).unwrap(), proposal);
    assert_eq!(
        proposal["payload"]["control_v2"]["actors"][0]["avatar_id"],
        "17"
    );
    let reply = commit(&mut host, &proposal, 1);
    assert_eq!(reply["kind"], "committed");
    let control = OwnedControlV2 {
        principal: DecimalId(17),
        avatar_id: DecimalId(17),
        control_generation: DecimalId(1),
        session_ref: format!("{:064x}", 17),
    };
    let actor = host.owned_control(&control).unwrap().actor;
    let saved = checkpoint(&host.committed).unwrap();
    host.committed.restore(&saved).unwrap();
    assert_eq!(host.owned_control(&control).unwrap().actor, actor);
    let request = hex(&encode_checkpoint_request(CheckpointRequest {
        id: 9,
        cursor: None,
    })
    .unwrap());
    let packet = host.player_checkpoint(control.clone(), request).unwrap();
    assert!(
        packet["packet_hex"]
            .as_str()
            .unwrap()
            .starts_with("574c5231")
    );
    let mut wrong = control.clone();
    wrong.avatar_id = DecimalId(18);
    assert!(host.owned_control(&wrong).is_err());
    wrong = control.clone();
    wrong.session_ref = "1".repeat(64);
    assert!(host.owned_control(&wrong).is_err());
    let b = host.handle(prepare(&host, 18, 2)).unwrap();
    commit(&mut host, &b, 2);
    assert_eq!(actors(&host.committed).as_array().unwrap().len(), 2);
}
#[test]
fn rejection_and_unknown_resolution_never_publish_candidate_actors() {
    let mut host = host();
    let before = checkpoint(&host.committed).unwrap();
    let proposal = host.handle(prepare(&host, 17, 1)).unwrap();
    let unknown = host
        .handle(RequestV1::Resolve {
            batch_id: DecimalId(1),
            digest: proposal["digest"].as_str().unwrap().into(),
            decision: DecisionV1::Unknown,
        })
        .unwrap();
    assert_eq!(unknown, proposal);
    assert_eq!(checkpoint(&host.committed).unwrap(), before);
    host.handle(RequestV1::Resolve {
        batch_id: DecimalId(1),
        digest: proposal["digest"].as_str().unwrap().into(),
        decision: DecisionV1::Rejected,
    })
    .unwrap();
    assert_eq!(checkpoint(&host.committed).unwrap(), before);
    assert!(
        host.handle(RequestV1::Prepare {
            batch_id: DecimalId(2),
            binding: host.binding.clone(),
            predecessor: crate::runtime::cursor(&host.committed).unwrap(),
            commands_hex: "00".into()
        })
        .is_err()
    );
}
#[test]
fn forged_export_and_stale_policy_do_not_allocate_a_principal() {
    let mut host = host();
    let mut request = prepare(&host, 17, 1);
    if let RequestV1::PrepareOwnedAdmission { export, .. } = &mut request {
        export.portable_json.push(' ');
    }
    let before = checkpoint(&host.committed).unwrap();
    assert!(host.handle(request).is_err());
    assert_eq!(checkpoint(&host.committed).unwrap(), before);
    let mut request = prepare(&host, 17, 2);
    if let RequestV1::PrepareOwnedAdmission { export, .. } = &mut request {
        export.permission_binding.revision = DecimalId(2);
        let mut value = serde_json::to_value(&*export).unwrap();
        value.as_object_mut().unwrap().remove("export_digest");
        export.export_digest = Digest::of(&serde_json::to_vec(&value).unwrap())
            .as_str()
            .into();
    }
    assert!(host.handle(request).is_err());
    assert_eq!(checkpoint(&host.committed).unwrap(), before);
}

#[test]
fn source_location_is_distinct_from_db_lot_id_and_bootstrap_needs_appearance() {
    let mut host = host();
    let proposal = host.handle(prepare(&host, 17, 1)).unwrap();
    commit(&mut host, &proposal, 1);
    let control = OwnedControlV2 {
        principal: DecimalId(17),
        avatar_id: DecimalId(17),
        control_generation: DecimalId(1),
        session_ref: format!("{:064x}", 17),
    };
    let binding = PlayerBindingV1 {
        source_epoch: DecimalId(1),
        lot_incarnation: DecimalId(1),
        lot_location: 16777472,
        avatar_id: 17,
    };
    assert!(
        host.player_bootstrap(control.clone(), binding.clone())
            .unwrap_err()
            .to_string()
            .contains("appearance")
    );
    host.appearance = Some(
        wonderland_game_runtime::world_view::WorldDocument::from_blueprint_xml(
            "<house><size>8</size><world><floors/><walls/></world><objects/></house>",
            "test:owned-host",
            "authored-fixture",
        )
        .unwrap(),
    );
    let packet = host
        .player_bootstrap(control.clone(), binding.clone())
        .unwrap();
    assert!(
        packet["packet_hex"]
            .as_str()
            .unwrap()
            .starts_with("574c4231")
    );
    let mut wrong = binding;
    wrong.lot_location = 41;
    assert!(host.player_bootstrap(control, wrong).is_err());
}

fn tick_request(host: &Host, batch: u64) -> Value {
    json!({"kind":"prepare_player_tick","batch_id":DecimalId(batch),"binding":host.binding,"predecessor":crate::runtime::cursor(&host.committed).unwrap(),"expected_control_revision":"1"})
}

fn acknowledge_export(host: &mut Host, exported: &Value) -> ReceiptCheckpointV1 {
    let p = &exported["payload"];
    let ack: ReceiptCheckpointV1 = serde_json::from_value(json!({
        "schema_version":1,"status":"persisted","binding":p["binding"],
        "cursor":p["cursor"],"checkpoint_sha256":p["checkpoint_sha256"],
        "through_batch_id":p["through_batch_id"],"receipt_set_sha256":exported["digest"]
    }))
    .unwrap();
    let response = host
        .handle(RequestV1::AcknowledgeReceipts {
            acknowledgement: ack.clone(),
        })
        .unwrap();
    assert_eq!(response["kind"], "receipts_retired");
    ack
}

#[test]
fn receipt_retirement_preserves_exact_committed_and_rejected_retry_binding() {
    let mut host = host();
    let first = tick_request(&host, 1);
    let proposal = host
        .handle(serde_json::from_value(first.clone()).unwrap())
        .unwrap();
    let committed = commit(&mut host, &proposal, 1);
    let second = tick_request(&host, 3); // The retired watermark also fences unused IDs.
    let proposal = host
        .handle(serde_json::from_value(second.clone()).unwrap())
        .unwrap();
    let rejected = host
        .handle(RequestV1::Resolve {
            batch_id: DecimalId(3),
            digest: proposal["digest"].as_str().unwrap().into(),
            decision: DecisionV1::Rejected,
        })
        .unwrap();
    let snapshot = checkpoint(&host.committed).unwrap();
    let exported = host.handle(RequestV1::ExportReceipts).unwrap();
    assert_eq!(exported["payload"]["records"][0]["reply"], committed);
    assert_eq!(exported["payload"]["records"][1]["reply"], rejected);
    assert_eq!(
        exported["payload"]["records"][0]["input_sha256"],
        json!(Digest::of(&serde_json::to_vec(&first).unwrap()))
    );
    let ack = acknowledge_export(&mut host, &exported);
    let inspected = host.handle(RequestV1::Inspect).unwrap();
    assert_eq!(inspected["retained_receipts"], 0);
    assert_eq!(inspected["retained_receipt_bytes"], 0);
    assert_eq!(inspected["retired_through_batch_id"], "3");
    let retried = host
        .handle(serde_json::from_value(first.clone()).unwrap())
        .unwrap();
    assert_eq!(retried["kind"], "retired");
    assert_eq!(
        retried["input_sha256"],
        exported["payload"]["records"][0]["input_sha256"]
    );
    let mut changed = first;
    changed["expected_control_revision"] = json!("99");
    let changed = host
        .handle(serde_json::from_value(changed).unwrap())
        .unwrap();
    assert_eq!(changed["kind"], "retired");
    assert_ne!(changed["input_sha256"], retried["input_sha256"]);
    for id in 1..=3 {
        assert_eq!(
            host.handle(RequestV1::Lookup {
                batch_id: DecimalId(id)
            })
            .unwrap()["kind"],
            "retired"
        );
    }
    let resolved = host
        .handle(RequestV1::Resolve {
            batch_id: DecimalId(3),
            digest: rejected["digest"].as_str().unwrap().into(),
            decision: DecisionV1::Committed {
                cursor: crate::runtime::cursor(&host.committed).unwrap(),
            },
        })
        .unwrap();
    assert_eq!(resolved["kind"], "retired");
    assert_eq!(resolved["digest"], rejected["digest"]);
    assert_eq!(checkpoint(&host.committed).unwrap(), snapshot);
    assert!(
        host.handle(RequestV1::AcknowledgeReceipts {
            acknowledgement: ack
        })
        .is_ok()
    );
}

#[test]
fn receipt_retirement_rejects_stale_or_forged_ack_and_never_discards_pending() {
    let mut host = host();
    let request = tick_request(&host, 1);
    let proposal = host
        .handle(serde_json::from_value(request).unwrap())
        .unwrap();
    commit(&mut host, &proposal, 1);
    let exported = host.handle(RequestV1::ExportReceipts).unwrap();
    let p = &exported["payload"];
    let ack: ReceiptCheckpointV1 = serde_json::from_value(json!({"schema_version":1,"status":"persisted","binding":p["binding"],"cursor":p["cursor"],"checkpoint_sha256":p["checkpoint_sha256"],"through_batch_id":p["through_batch_id"],"receipt_set_sha256":exported["digest"]})).unwrap();
    for index in 0..5 {
        let mut bad = ack.clone();
        match index {
            0 => bad.receipt_set_sha256 = "0".repeat(64),
            1 => bad.checkpoint_sha256 = "0".repeat(64),
            2 => bad.cursor.completed_tick.0 += 1,
            3 => bad.binding.lot_incarnation.0 += 1,
            _ => bad.through_batch_id.0 += 1,
        }
        assert!(
            host.handle(RequestV1::AcknowledgeReceipts {
                acknowledgement: bad
            })
            .is_err()
        );
        assert_eq!(
            host.handle(RequestV1::Inspect).unwrap()["retained_receipts"],
            1
        );
    }
    let request = tick_request(&host, 2);
    let pending = host
        .handle(serde_json::from_value(request).unwrap())
        .unwrap();
    assert!(host.handle(RequestV1::ExportReceipts).is_err());
    assert!(
        host.handle(RequestV1::AcknowledgeReceipts {
            acknowledgement: ack.clone()
        })
        .is_err()
    );
    assert_eq!(
        host.handle(RequestV1::Resolve {
            batch_id: DecimalId(2),
            digest: pending["digest"].as_str().unwrap().into(),
            decision: DecisionV1::Unknown
        })
        .unwrap(),
        pending
    );
    assert_eq!(
        host.handle(RequestV1::Lookup {
            batch_id: DecimalId(2)
        })
        .unwrap(),
        pending
    );
    commit(&mut host, &pending, 2);
    assert!(
        host.handle(RequestV1::AcknowledgeReceipts {
            acknowledgement: ack
        })
        .is_err()
    );
    let fresh = host.handle(RequestV1::ExportReceipts).unwrap();
    acknowledge_export(&mut host, &fresh);
}

#[test]
fn receipt_export_pages_exact_prefix_without_losing_later_decisions() {
    let mut host = host();
    for batch in 1..=65 {
        let request = tick_request(&host, batch);
        let proposal = host
            .handle(serde_json::from_value(request).unwrap())
            .unwrap();
        commit(&mut host, &proposal, batch);
    }
    let first = host.handle(RequestV1::ExportReceipts).unwrap();
    assert_eq!(first["payload"]["records"].as_array().unwrap().len(), 64);
    assert_eq!(first["payload"]["through_batch_id"], "64");
    acknowledge_export(&mut host, &first);
    assert_eq!(
        host.handle(RequestV1::Lookup {
            batch_id: DecimalId(65)
        })
        .unwrap()["kind"],
        "committed"
    );
    let second = host.handle(RequestV1::ExportReceipts).unwrap();
    assert_eq!(second["payload"]["records"].as_array().unwrap().len(), 1);
    acknowledge_export(&mut host, &second);
    assert_eq!(
        host.handle(RequestV1::Inspect).unwrap()["retained_receipts"],
        0
    );
}

#[test]
fn checkpoint_acknowledgements_keep_host_receipts_bounded_beyond_4096_batches() {
    let mut host = host();
    for batch in 1..=4160 {
        let request = tick_request(&host, batch);
        let proposal = host
            .handle(serde_json::from_value(request).unwrap())
            .unwrap();
        assert_eq!(proposal["kind"], "proposal");
        commit(&mut host, &proposal, batch);
        if batch % 32 == 0 {
            let exported = host.handle(RequestV1::ExportReceipts).unwrap();
            assert_eq!(exported["payload"]["records"].as_array().unwrap().len(), 32);
            acknowledge_export(&mut host, &exported);
        }
    }
    let inspected = host.handle(RequestV1::Inspect).unwrap();
    assert_eq!(inspected["retired_through_batch_id"], "4160");
    assert_eq!(inspected["retained_receipts"], 0);
    assert_eq!(inspected["retained_receipt_bytes"], 0);
}

fn player_action_input(host: &Host, batch: u64, action: &Value) -> Value {
    json!({"kind":"prepare_player_action","batch_id":DecimalId(batch),"binding":host.binding,
        "predecessor":crate::runtime::cursor(&host.committed).unwrap(),"expected_control_revision":"1",
        "control":action["control"],"action_hex":action["action_hex"]})
}

#[test]
fn action_evidence_is_digest_bound_before_commit_and_preserves_original_observation() {
    let mut host = host();
    let admission = host.handle(prepare(&host, 17, 1)).unwrap();
    commit(&mut host, &admission, 1);
    let action = action_content::player_action(&host, 17, 1, false).unwrap();
    // Revalidation may refresh the command's observed state, but the receipt
    // must still echo the exact bytes originally submitted by this player.
    host.committed.advance(vec![]).unwrap();
    let original = player_action_input(&host, 2, &action);
    let before = checkpoint(&host.committed).unwrap();
    let proposal = host
        .handle(serde_json::from_value(original.clone()).unwrap())
        .unwrap();
    assert_eq!(proposal["kind"], "proposal", "{proposal}");
    let evidence = &proposal["payload"]["action_v1"];
    assert_eq!(evidence["schema_version"], 1);
    assert_eq!(evidence["request_hex"], action["action_hex"]);
    assert_eq!(evidence["command_sequence"], "1");
    assert_eq!(evidence["control"]["account_id"], "1017");
    assert_eq!(evidence["control"]["principal"], "17");
    assert_eq!(
        evidence["input_sha256"],
        json!(Digest::of(&serde_json::to_vec(&original).unwrap()))
    );
    assert_eq!(
        evidence["request_sha256"],
        json!(Digest::of(
            &unhex(action["action_hex"].as_str().unwrap()).unwrap()
        ))
    );
    assert_eq!(
        proposal["digest"],
        json!(Digest::of(
            &serde_json::to_vec(&proposal["payload"]).unwrap()
        ))
    );
    let mut tampered = proposal["payload"].clone();
    tampered["action_v1"]["request_sha256"] = json!("0".repeat(64));
    assert_ne!(
        proposal["digest"],
        json!(Digest::of(&serde_json::to_vec(&tampered).unwrap()))
    );
    assert_eq!(checkpoint(&host.committed).unwrap(), before);
    let committed = commit(&mut host, &proposal, 2);
    assert_eq!(committed["receipt_hex"], evidence["accepted_receipt_hex"]);
    assert_ne!(committed["receipt_hex"], evidence["rejected_receipt_hex"]);
    let exported = host.handle(RequestV1::ExportReceipts).unwrap();
    assert_eq!(
        exported["payload"]["records"][1]["input_sha256"],
        evidence["input_sha256"]
    );
    assert_eq!(exported["payload"]["records"][1]["reply"], committed);
}

#[test]
fn cancel_evidence_has_exact_terminal_variants_and_rejection_does_not_consume_vm_sequence() {
    let mut host = host();
    let admission = host.handle(prepare(&host, 17, 1)).unwrap();
    commit(&mut host, &admission, 1);
    let action = action_content::player_action(&host, 17, 1, false).unwrap();
    let input = player_action_input(&host, 2, &action);
    let proposal = host.handle(serde_json::from_value(input).unwrap()).unwrap();
    assert_eq!(proposal["kind"], "proposal", "{proposal}");
    commit(&mut host, &proposal, 2);
    let action = action_content::player_action(&host, 17, 2, true).unwrap();
    let input = player_action_input(&host, 3, &action);
    let proposal = host.handle(serde_json::from_value(input).unwrap()).unwrap();
    assert_eq!(proposal["kind"], "proposal", "{proposal}");
    let before = checkpoint(&host.committed).unwrap();
    let rejected = host
        .handle(RequestV1::Resolve {
            batch_id: DecimalId(3),
            digest: proposal["digest"].as_str().unwrap().into(),
            decision: DecisionV1::Rejected,
        })
        .unwrap();
    assert_eq!(
        rejected["receipt_hex"],
        proposal["payload"]["action_v1"]["rejected_receipt_hex"]
    );
    assert_eq!(checkpoint(&host.committed).unwrap(), before);
    host.committed.advance(vec![]).unwrap();
    let next = action_content::player_action(&host, 17, 2, true).unwrap();
    assert_ne!(next["action_hex"], action["action_hex"]);
    let input = player_action_input(&host, 4, &next);
    let next = host.handle(serde_json::from_value(input).unwrap()).unwrap();
    assert_eq!(next["kind"], "proposal", "{next}");
    assert_eq!(next["payload"]["action_v1"]["command_sequence"], "2");
    let committed = commit(&mut host, &next, 4);
    assert_eq!(
        committed["receipt_hex"],
        next["payload"]["action_v1"]["accepted_receipt_hex"]
    );
}

#[test]
fn ticks_and_admissions_keep_legacy_payload_without_action_evidence() {
    let mut host = host();
    let admission = host.handle(prepare(&host, 17, 1)).unwrap();
    assert!(admission["payload"].get("action_v1").is_none());
    commit(&mut host, &admission, 1);
    let request = tick_request(&host, 2);
    let tick = host
        .handle(serde_json::from_value(request).unwrap())
        .unwrap();
    assert!(tick["payload"].get("action_v1").is_none());
}
