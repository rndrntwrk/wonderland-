//! Typed boundary harnesses, not original content qualification.
use super::*;
use wonderland_game_runtime::{
    GameRuntime, LotModel, RuntimeConfig, RuntimeRole, VmMode,
    live_wire::{CheckpointRequest, encode_checkpoint_request},
    sim_core::{avatars::state::TEMPLATE_PERSON, lot_permissions::LotPermissionState},
};
#[path = "../examples/support/owned_action_content.rs"]
mod action_content;

#[path = "../../../crates/game-runtime/tests/support/source_terrain.rs"]
mod source_terrain;

fn host() -> Host {
    host_with_content(action_content::content())
}
fn host_with_content(content: wonderland_game_runtime::sim_core::state::ContentSet) -> Host {
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

fn preparation_status(host: &mut Host) -> Value {
    let before = host.handle(RequestV1::Inspect).unwrap();
    let mut expected = before.clone();
    expected.as_object_mut().unwrap().remove("checkpoint_hex");
    expected["kind"] = json!(if before["recovery_pending"] == true {
        "recovery_status"
    } else {
        "committed_status"
    });
    let request = serde_json::from_value(json!({"kind":"inspect_status"}))
        .expect("additive preparation-status request must decode");
    let actual = host.handle(request).unwrap();
    assert_eq!(
        actual, expected,
        "only checkpoint and response kind may differ"
    );
    assert!(actual.get("checkpoint_hex").is_none());
    assert_eq!(
        host.handle(RequestV1::Inspect).unwrap(),
        before,
        "status must be read-only"
    );
    actual
}

#[test]
fn preparation_status_preserves_full_metadata_through_transaction_and_retirement() {
    let mut host = host();
    let initial = preparation_status(&mut host);
    let request = tick_request(&host, 1);
    let proposal = host
        .handle(serde_json::from_value(request).unwrap())
        .unwrap();
    let pending = preparation_status(&mut host);
    assert_eq!(pending["cursor"], initial["cursor"]);
    assert_eq!(pending["pending_batch"], "1");
    commit(&mut host, &proposal, 1);
    let committed = preparation_status(&mut host);
    assert_ne!(committed["cursor"], initial["cursor"]);
    assert_eq!(committed["retained_receipts"], 1);
    let request = tick_request(&host, 2);
    let proposal = host
        .handle(serde_json::from_value(request).unwrap())
        .unwrap();
    host.handle(RequestV1::Resolve {
        batch_id: DecimalId(2),
        digest: proposal["digest"].as_str().unwrap().into(),
        decision: DecisionV1::Rejected,
    })
    .unwrap();
    let rejected = preparation_status(&mut host);
    assert_eq!(rejected["cursor"], committed["cursor"]);
    assert_eq!(rejected["retained_receipts"], 2);
    let exported = host.handle(RequestV1::ExportReceipts).unwrap();
    acknowledge_export(&mut host, &exported);
    let retired = preparation_status(&mut host);
    assert_eq!(retired["retained_receipts"], 0);
    assert_eq!(retired["retained_receipt_bytes"], 0);
    assert_eq!(retired["retired_through_batch_id"], "2");
}

#[test]
fn preparation_status_does_not_activate_recovery_or_validate_stale_transactions() {
    let mut host = host();
    let before = preparation_status(&mut host);
    let mut stale = tick_request(&host, 1);
    stale["predecessor"]["state_hash"] = json!("0".repeat(64));
    assert!(host.handle(serde_json::from_value(stale).unwrap()).is_err());
    assert_eq!(preparation_status(&mut host), before);
    host.recovery_committed = false;
    let recovering = preparation_status(&mut host);
    assert_eq!(recovering["kind"], "recovery_status");
    assert_eq!(recovering["recovery_pending"], true);
    let request = tick_request(&host, 1);
    assert!(
        host.handle(serde_json::from_value(request).unwrap())
            .is_err()
    );
    assert_eq!(preparation_status(&mut host), recovering);
    host.recovery_committed = true;
    assert_eq!(preparation_status(&mut host), before);
}

#[test]
fn preparation_status_request_is_additive_and_rejects_payload_fields() {
    assert!(serde_json::from_value::<RequestV1>(json!({"kind":"inspect"})).is_ok());
    assert!(serde_json::from_value::<RequestV1>(json!({"kind":"inspect_status"})).is_ok());
    for extra in ["checkpoint_hex", "commands_hex", "decision", "control"] {
        let mut request = json!({"kind":"inspect_status"});
        request[extra] = json!("not an input to a read-only status request");
        assert!(serde_json::from_value::<RequestV1>(request).is_err());
    }
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
    assert!(next["payload"].get("departure_v1").is_none());
    let events: Vec<crate::events::EventEnvelope> = {
        use bincode::Options;
        bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .deserialize(&unhex(next["payload"]["events_hex"].as_str().unwrap()).unwrap())
            .unwrap()
    };
    let actor = host
        .committed
        .sim()
        .state()
        .owned_admission
        .actors()
        .unwrap()[&17]
        .actor;
    assert!(events.iter().any(|event| matches!(event.event,
        crate::events::SupervisorEvent::OwnedEodDisconnectRequested { actor: found, control_generation: 1 } if found == actor)));
    let committed = commit(&mut host, &next, 4);
    assert!(
        host.committed
            .sim()
            .state()
            .owned_departures
            .eod_disconnect_requests
            .contains(&(actor, 1))
    );
    assert!(
        host.committed
            .sim()
            .state()
            .owned_departures
            .records
            .is_empty()
    );
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

#[test]
fn source_terrain_action_is_owned_commit_only_and_echoes_exact_request() {
    use wonderland_game_runtime::{
        TilePos,
        live_wire::player::{PlayerAction, encode_action},
        sim_core::runtime::goto_adapter::SourceGotoIntent,
    };
    let mut host = host_with_content(source_terrain::content());
    let admission = host.handle(prepare(&host, 17, 1)).unwrap();
    commit(&mut host, &admission, 1);
    let owned = host
        .committed
        .sim()
        .state()
        .owned_admission
        .actors()
        .unwrap()[&17]
        .clone();
    let position = TilePos::new(4, 4, 1).center();
    let batch = host
        .committed
        .sim()
        .terrain_offers(PrincipalKey(17), owned.actor, position)
        .unwrap();
    let bytes = encode_action(&PlayerAction::Goto(SourceGotoIntent {
        principal: PrincipalKey(17),
        actor: batch.actor,
        world_revision: batch.world_revision,
        queue_revision: batch.queue_revision,
        command_sequence: 1,
        x: 72,
        y: 72,
        level: 1,
        interaction: 7,
        param0: 0,
    }))
    .unwrap();
    let action = json!({"control":{
        "principal":"17", "avatar_id":"17", "control_generation":"1", "session_ref":hex(&owned.session_ref)
    }, "action_hex":hex(&bytes)});
    // Real latency is revalidated under the host lock; evidence keeps submitted bytes.
    host.committed.advance(vec![]).unwrap();
    let before = checkpoint(&host.committed).unwrap();
    let original = player_action_input(&host, 2, &action);
    let proposal = host
        .handle(serde_json::from_value(original).unwrap())
        .unwrap();
    assert_eq!(proposal["kind"], "proposal", "{proposal}");
    assert_eq!(
        proposal["payload"]["action_v1"]["request_hex"],
        action["action_hex"]
    );
    assert_eq!(proposal["payload"]["action_v1"]["command_sequence"], "1");
    assert_eq!(checkpoint(&host.committed).unwrap(), before);
    commit(&mut host, &proposal, 2);
    assert_eq!(
        host.committed.sim().state().interaction_queues[&owned.actor.object_id]
            .last_command_sequence(),
        Some(1)
    );
    assert!(
        host.committed
            .sim()
            .state()
            .entities
            .values()
            .any(|e| e.info.guid
                == wonderland_game_runtime::sim_core::runtime::goto_adapter::SOURCE_GOTO_GUID)
    );
    let control = serde_json::from_value(action["control"].clone()).unwrap();
    assert!(host.action_command(&control, &bytes).is_err());
    let mut wrong = control;
    wrong.principal = DecimalId(18);
    assert!(host.action_command(&wrong, &bytes).is_err());
}

// Authored TTAB/routine boundary harness. It exercises actual accepted mode25
// execution and is deliberately not qualification of original source content.
fn departure_content(detach: bool) -> wonderland_game_runtime::sim_core::state::ContentSet {
    use wonderland_game_runtime::sim_core::{
        interactions::{
            ActionFlags, InteractionDefinition, InteractionKey, InteractionScope, PermissionFlags,
            RoutineBinding,
        },
        state::ContentSet,
        vm::{RoutineKey, RoutineScope, VmInstruction, VmRoutine},
    };
    let base = action_content::content();
    let mut routines = base.routines().clone();
    routines.bind_semiglobal(TEMPLATE_PERSON, 900).unwrap();
    let instruction = if detach {
        VmInstruction::new(1, 254, 255, [25, 0, 0, 0, 0, 0, 0, 0])
    } else {
        VmInstruction::new(0, 0, 255, [0; 8])
    };
    routines
        .insert(
            RoutineKey {
                scope: RoutineScope::SemiGlobal(900),
                id: 8373,
            },
            VmRoutine::new(8373, 0, 4, vec![instruction]).unwrap(),
        )
        .unwrap();
    let mut table = base.interaction_table(TEMPLATE_PERSON).unwrap().clone();
    table.definitions.push(InteractionDefinition {
        key: InteractionKey {
            tta_index: 173,
            scope: InteractionScope::Local,
        },
        action: RoutineBinding {
            routine_id: 8373,
            code_owner_guid: TEMPLATE_PERSON,
        },
        check: None,
        flags: ActionFlags(ActionFlags::ALLOW_VISITORS | ActionFlags::ALLOW_CONSECUTIVE),
        permissions: PermissionFlags::default(),
        label: Some("Authored owned departure boundary".into()),
    });
    // This authored two-peer boundary fixture has no mailbox. Permit both
    // avatars at the source join fallback tile so the peer can actually act.
    let mut person = base.object(TEMPLATE_PERSON).unwrap().clone();
    person.object_data[8] |= 1 << 4; // source AllowPersonIntersection
    ContentSet::new(routines, vec![person], vec![], base.tuning().clone())
        .unwrap()
        .with_interaction_tables(vec![(TEMPLATE_PERSON, table)])
        .unwrap()
}
fn departure_host(content: wonderland_game_runtime::sim_core::state::ContentSet) -> Host {
    let mut host = host_with_content(content);
    host.binding.source_pin = SOURCE_LOT_SAVE_PIN.into();
    for (id, batch) in [(17, 1), (18, 2)] {
        let proposal = host.handle(prepare(&host, id, batch)).unwrap();
        commit(&mut host, &proposal, batch);
    }
    host
}
fn drain_request(host: &Host, principal: u64, batch: u64) -> Value {
    let control = &host
        .committed
        .sim()
        .state()
        .owned_admission
        .actors()
        .unwrap()[&principal];
    let identity = DepartureIdentityV1 {
        authority_id: "b".repeat(64),
        account_id: DecimalId(control.account_id),
        avatar_id: DecimalId(u64::from(control.avatar_id.0)),
        lot_id: DecimalId(41),
        control_generation: DecimalId(control.control_generation),
        session_ref: hex(&control.session_ref),
        admission_operation: control.admission_operation.clone(),
        reservation_digest: hex(&control.reservation_digest),
        export_digest: hex(&control.export_digest),
        principal: DecimalId(control.principal),
        actor: DepartureEntityRefV1 {
            object_id: u16::try_from(control.actor.object_id.0).unwrap(),
            generation: control.actor.generation,
        },
    };
    let operation = departure_operation_v1(&identity).unwrap();
    json!({"kind":"prepare_owned_drain","batch_id":DecimalId(batch),"binding":host.binding,
        "predecessor":crate::runtime::cursor(&host.committed).unwrap(),"expected_control_revision":"42",
        "departure":{"schema_version":1,"identity":identity,"departure_operation":operation},
        "fence":{"version":1,"authority":identity.authority_id,"avatar_id":identity.avatar_id,
            "generation":identity.control_generation,"lot_id":identity.lot_id,"request_digest":identity.reservation_digest}})
}
#[test]
fn owned_drain_rejects_mismatched_fence_and_installed_identity_without_preparing() {
    let mut host = departure_host(action_content::content());
    let request = drain_request(&host, 17, 3);
    let before = checkpoint(&host.committed).unwrap();
    for (section, field, value) in [
        ("fence", "generation", json!("2")),
        ("fence", "request_digest", json!("a".repeat(64))),
        ("identity", "account_id", json!("999")),
        ("identity", "session_ref", json!("a".repeat(64))),
        ("identity", "export_digest", json!("a".repeat(64))),
    ] {
        let mut invalid = request.clone();
        if section == "identity" {
            invalid["departure"]["identity"][field] = value;
        } else {
            invalid[section][field] = value;
        }
        assert!(
            host.handle(serde_json::from_value(invalid).unwrap())
                .is_err()
        );
        assert_eq!(checkpoint(&host.committed).unwrap(), before);
        assert!(host.handle(RequestV1::Inspect).unwrap()["pending_batch"].is_null());
    }
    let mut injected = request;
    injected["commands_hex"] = json!("00");
    assert!(serde_json::from_value::<RequestV1>(injected).is_err());
}
#[test]
fn blocked_owned_drain_is_real_fenced_progress_without_fabricated_save_or_detach() {
    let mut host = departure_host(action_content::content());
    let before = checkpoint(&host.committed).unwrap();
    let request = drain_request(&host, 17, 3);
    let proposal = host
        .handle(serde_json::from_value(request.clone()).unwrap())
        .unwrap();
    assert_eq!(checkpoint(&host.committed).unwrap(), before);
    assert_eq!(
        host.handle(serde_json::from_value(request.clone()).unwrap())
            .unwrap(),
        proposal
    );
    let evidence: DepartureEvidenceV1 =
        serde_json::from_value(proposal["payload"]["departure_v1"].clone()).unwrap();
    assert_eq!(
        evidence.lifecycle.status,
        DepartureLifecycleStatusV1::Blocked
    );
    assert_eq!(
        evidence.lifecycle.blocker.as_deref(),
        Some("missing_source_action")
    );
    assert!(evidence.save.is_none());
    assert!(evidence.detach.is_none());
    assert_eq!(proposal["payload"]["control_v2"]["revision_after"], "42");
    let reply = commit(&mut host, &proposal, 3);
    assert_eq!(reply["departure_v1"], proposal["payload"]["departure_v1"]);
    assert_eq!(
        host.handle(serde_json::from_value(request).unwrap())
            .unwrap(),
        reply
    );
    assert!(
        host.handle(serde_json::from_value(tick_request(&host, 4)).unwrap())
            .is_err()
    );
    let later = host
        .handle(serde_json::from_value(drain_request(&host, 17, 4)).unwrap())
        .unwrap();
    assert_eq!(
        later["payload"]["departure_v1"]["lifecycle"]["status"],
        "blocked"
    );
    assert!(later["payload"]["departure_v1"]["save"].is_null());
    assert_eq!(actors(&host.committed).as_array().unwrap().len(), 2);
}
#[test]
fn owned_drain_captures_once_and_later_leaving_tick_does_not_reemit_save() {
    let mut host = departure_host(departure_content(false));
    let first = host
        .handle(serde_json::from_value(drain_request(&host, 17, 3)).unwrap())
        .unwrap();
    assert!(first["payload"]["departure_v1"]["save"].is_object());
    assert!(first["payload"]["departure_v1"]["detach"].is_null());
    commit(&mut host, &first, 3);
    let second = host
        .handle(serde_json::from_value(drain_request(&host, 17, 4)).unwrap())
        .unwrap();
    assert!(second["payload"]["departure_v1"]["save"].is_null());
    assert_eq!(
        second["payload"]["departure_v1"]["lifecycle"]["status"],
        "leaving"
    );
    assert_eq!(second["payload"]["control_v2"]["revision_after"], "42");
}
#[test]
fn source_owned_drain_proves_save_before_removal_and_retains_exact_receipt() {
    use crate::events::{EventEnvelope, SupervisorEvent};
    use bincode::Options;
    let mut host = departure_host(departure_content(true));
    let other = host
        .committed
        .sim()
        .state()
        .owned_admission
        .actors()
        .unwrap()[&18]
        .clone();
    let before = checkpoint(&host.committed).unwrap();
    let request = drain_request(&host, 17, 3);
    let proposal = host
        .handle(serde_json::from_value(request.clone()).unwrap())
        .unwrap();
    let evidence: DepartureEvidenceV1 =
        serde_json::from_value(proposal["payload"]["departure_v1"].clone()).unwrap();
    assert_eq!(
        evidence.lifecycle.status,
        DepartureLifecycleStatusV1::Detached
    );
    let save = evidence.save.as_ref().unwrap();
    let detach = evidence.detach.as_ref().unwrap();
    assert_eq!(detach.removal_reason, DepartureRemovalReasonV1::SourceLeave);
    assert!(save.event_ordinal < detach.event_ordinal);
    assert_eq!(
        crate::departure::source_avatar_lot_save_sha256(&save.save_record).unwrap(),
        save.save_sha256
    );
    assert_eq!(proposal["payload"]["control_v2"]["revision_before"], "42");
    assert_eq!(proposal["payload"]["control_v2"]["revision_after"], "43");
    let retained: Vec<EventEnvelope> = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .deserialize(&unhex(proposal["payload"]["events_hex"].as_str().unwrap()).unwrap())
        .unwrap();
    assert!(
        retained
            .iter()
            .any(|event| event.ordinal == u32::from(save.event_ordinal)
                && matches!(event.event, SupervisorEvent::OwnedDepartureCaptured { .. }))
    );
    assert!(
        retained
            .iter()
            .any(|event| event.ordinal == u32::from(detach.event_ordinal)
                && matches!(event.event, SupervisorEvent::OwnedDepartureDetached { .. }))
    );
    assert_eq!(checkpoint(&host.committed).unwrap(), before);
    let committed = commit(&mut host, &proposal, 3);
    assert_eq!(
        committed["departure_v1"],
        proposal["payload"]["departure_v1"]
    );
    assert_eq!(
        host.committed
            .sim()
            .state()
            .owned_admission
            .actors()
            .unwrap()[&18],
        other
    );
    assert_eq!(
        host.handle(serde_json::from_value(request).unwrap())
            .unwrap(),
        committed
    );
    assert_eq!(
        host.handle(RequestV1::Lookup {
            batch_id: DecimalId(3)
        })
        .unwrap(),
        committed
    );
    let exported = host.handle(RequestV1::ExportReceipts).unwrap();
    assert_eq!(exported["payload"]["records"][2]["reply"], committed);
    let ack = acknowledge_export(&mut host, &exported);
    assert_eq!(ack.through_batch_id, DecimalId(3));
    assert_eq!(
        host.handle(RequestV1::Lookup {
            batch_id: DecimalId(3)
        })
        .unwrap()["kind"],
        "retired"
    );
}
#[test]
fn rejected_owned_drain_does_not_publish_a_capture_or_actor_removal() {
    let mut host = departure_host(departure_content(true));
    let before = checkpoint(&host.committed).unwrap();
    let request = drain_request(&host, 17, 3);
    let proposal = host
        .handle(serde_json::from_value(request.clone()).unwrap())
        .unwrap();
    let reply = host
        .handle(RequestV1::Resolve {
            batch_id: DecimalId(3),
            digest: proposal["digest"].as_str().unwrap().into(),
            decision: DecisionV1::Rejected,
        })
        .unwrap();
    assert_eq!(reply["kind"], "rejected");
    assert!(reply.get("departure_v1").is_none());
    assert_eq!(checkpoint(&host.committed).unwrap(), before);
    assert_eq!(
        host.handle(serde_json::from_value(request).unwrap())
            .unwrap(),
        reply
    );
    let retry = host
        .handle(serde_json::from_value(drain_request(&host, 17, 4)).unwrap())
        .unwrap();
    assert_eq!(
        retry["payload"]["departure_v1"],
        proposal["payload"]["departure_v1"]
    );
}

#[test]
fn ordinary_registry_validation_cannot_reuse_drain_only_removal_authority() {
    let mut host = departure_host(departure_content(true));
    let mut before = GameRuntime::new(
        host.committed.sim().content().clone(),
        host.committed.sim().state().world.lot.clone(),
        host.config.clone(),
        RuntimeRole::Authority,
    )
    .unwrap();
    before
        .restore(&checkpoint(&host.committed).unwrap())
        .unwrap();
    let proposal = host
        .handle(serde_json::from_value(drain_request(&host, 17, 3)).unwrap())
        .unwrap();
    let evidence: DepartureEvidenceV1 =
        serde_json::from_value(proposal["payload"]["departure_v1"].clone()).unwrap();
    commit(&mut host, &proposal, 3);
    assert!(crate::departure::control_revision_after(&before, &host.committed, 42, None).is_err());
    assert_eq!(
        crate::departure::control_revision_after(&before, &host.committed, 42, Some(&evidence))
            .unwrap(),
        43
    );
    let mut mismatched = evidence;
    mismatched.identity.principal = DecimalId(18);
    assert!(
        crate::departure::control_revision_after(&before, &host.committed, 42, Some(&mismatched))
            .is_err()
    );
}

#[test]
fn source_owned_drain_retains_peer_cancel_eod_request_without_peer_departure() {
    use crate::events::{EventEnvelope, SupervisorEvent};
    use bincode::Options;
    use wonderland_game_runtime::{
        InteractionIntent, QueryOptions,
        live_wire::player::{PlayerAction, encode_action},
    };
    let mut host = departure_host(departure_content(true));
    let target = host
        .committed
        .sim()
        .state()
        .owned_admission
        .actors()
        .unwrap()[&17]
        .clone();
    let peer = host
        .committed
        .sim()
        .state()
        .owned_admission
        .actors()
        .unwrap()[&18]
        .clone();
    let offers = host
        .committed
        .offers(
            PrincipalKey(18),
            peer.actor,
            target.actor,
            QueryOptions::default(),
        )
        .unwrap();
    let offer = offers
        .offers
        .iter()
        .find(|offer| offer.interaction.tta_index == 7)
        .expect("authored in-world peer has a waiting-action offer");
    let intent = PlayerAction::Invoke(InteractionIntent {
        principal: PrincipalKey(18),
        seen: offers.seen,
        queue_revision: host.committed.sim().state().interaction_queues[&peer.actor.object_id]
            .revision(),
        command_sequence: 1,
        interaction: offer.interaction,
        param0: offer.param0,
    });
    let action = json!({"control":{"principal":"18","avatar_id":"18","control_generation":"1",
        "session_ref":hex(&peer.session_ref)},"action_hex":hex(&encode_action(&intent).unwrap())});
    let invoke = host
        .handle(serde_json::from_value(player_action_input(&host, 3, &action)).unwrap())
        .unwrap();
    assert_eq!(invoke["kind"], "proposal", "{invoke}");
    commit(&mut host, &invoke, 3);
    let mut before = GameRuntime::new(
        host.committed.sim().content().clone(),
        host.committed.sim().state().world.lot.clone(),
        host.config.clone(),
        RuntimeRole::Authority,
    )
    .unwrap();
    before
        .restore(&checkpoint(&host.committed).unwrap())
        .unwrap();
    let input = drain_request(&host, 17, 4);
    let request: DepartureRequestV1 = serde_json::from_value(input["departure"].clone()).unwrap();
    let proposal = host.handle(serde_json::from_value(input).unwrap()).unwrap();
    assert_eq!(proposal["kind"], "proposal", "{proposal}");
    let bytes = unhex(proposal["payload"]["events_hex"].as_str().unwrap()).unwrap();
    let mut events: Vec<EventEnvelope> = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .deserialize(&bytes)
        .unwrap();
    assert!(events.iter().any(|event| matches!(event.event,
        SupervisorEvent::OwnedEodDisconnectRequested { actor, control_generation: 1 } if actor == peer.actor)));
    commit(&mut host, &proposal, 4);
    assert_eq!(
        host.committed
            .sim()
            .state()
            .owned_admission
            .actors()
            .unwrap()[&18],
        peer
    );
    assert!(
        !host
            .committed
            .sim()
            .state()
            .owned_departures
            .records
            .contains_key(&peer.actor)
    );
    assert!(
        host.committed
            .sim()
            .state()
            .owned_departures
            .eod_disconnect_requests
            .contains(&(peer.actor, 1))
    );
    crate::departure::departure_evidence(&before, &host.committed, &request, &events).unwrap();
    let peer_event = events
        .iter_mut()
        .find(|event| {
            matches!(event.event,
        SupervisorEvent::OwnedEodDisconnectRequested { actor, .. } if actor == peer.actor)
        })
        .unwrap();
    if let SupervisorEvent::OwnedEodDisconnectRequested {
        control_generation, ..
    } = &mut peer_event.event
    {
        *control_generation += 1;
    }
    assert!(
        crate::departure::departure_evidence(&before, &host.committed, &request, &events).is_err()
    );
    let mut events: Vec<EventEnvelope> = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .deserialize(&bytes)
        .unwrap();
    events.push(EventEnvelope {
        tick: host.committed.sim().state().completed_tick,
        ordinal: 65535,
        event: SupervisorEvent::OwnedDepartureCaptured {
            actor: peer.actor,
            control_generation: 1,
            tick: host.committed.sim().state().completed_tick,
        },
    });
    assert!(
        crate::departure::departure_evidence(&before, &host.committed, &request, &events).is_err()
    );
}

#[test]
fn inspect_placement_is_exact_committed_actor_state_not_a_custody_grant() {
    // Existing authored content has no mailbox or intersection allowance: its
    // second source-compatible admission remains OOW at the occupied fallback.
    let mut host = host();
    for (id, batch) in [(17, 1), (18, 2)] {
        let proposal = host.handle(prepare(&host, id, batch)).unwrap();
        commit(&mut host, &proposal, batch);
    }
    let before = checkpoint(&host.committed).unwrap();
    let inspected = host.handle(RequestV1::Inspect).unwrap();
    let actors = inspected["actors"].as_array().unwrap();
    let placements = inspected["actor_placements"].as_array().unwrap();
    assert_eq!(placements.len(), 2);
    for (actor, placement) in actors.iter().zip(placements) {
        for field in ["principal", "avatar_id", "control_generation", "actor"] {
            assert_eq!(actor[field], placement[field]);
        }
    }
    assert_eq!(placements[0]["in_world"], true);
    assert_eq!(placements[0]["position"], json!({"x":56,"y":56,"level":1}));
    assert_eq!(placements[1]["in_world"], false);
    assert_eq!(
        placements[1]["position"],
        json!({"x":-32768,"y":-32768,"level":1})
    );
    assert_eq!(checkpoint(&host.committed).unwrap(), before);
    assert_eq!(
        host.committed
            .sim()
            .state()
            .owned_admission
            .actors()
            .unwrap()
            .len(),
        2
    );
}
