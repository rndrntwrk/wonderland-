//! Typed boundary harnesses, not original content qualification.
use super::*;
use wonderland_game_runtime::{
    GameRuntime, LotModel, RuntimeConfig, RuntimeRole, VmMode,
    live_wire::{CheckpointRequest, encode_checkpoint_request},
    sim_core::{
        avatars::{motives::TsoMotiveTuning, state::TEMPLATE_PERSON},
        lot_permissions::LotPermissionState,
        state::{ContentSet, ObjectDefinition, TuningSet},
        vm::RoutineStore,
    },
};
fn host() -> Host {
    let content = ContentSet::new(
        RoutineStore::new(),
        vec![ObjectDefinition::new(TEMPLATE_PERSON, 0)],
        vec![],
        TuningSet {
            tso_motives: Some(TsoMotiveTuning {
                flat_sim: [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0],
                category_weights: [[1000; 7]; 11],
            }),
            ..Default::default()
        },
    )
    .unwrap();
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
