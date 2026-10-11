//! Controlled construction exchange executed natively and in ordinary WASM.
//! Source BHAV is unchanged; grants, prices and durable completion are authored fixtures.
#[allow(dead_code)]
#[path = "../tests/live_session/support.rs"]
mod support;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};
use wonderland_game_runtime::live_wire::construction::{
    exchange::{
        self,
        client::{Client, ClientStage},
    },
    session::ConstructionSession,
    *,
};
use wonderland_game_runtime::sim_core::world::build::*;
use wonderland_game_runtime::{live_wire::player::PlayerBinding, *};
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn run() -> Vec<u8> {
    let (mut server, mut replica, actor) = support::pair_with_content(
        support::content("cursebook_set_permission.iff", 4107, false),
        true,
    );
    let binding = PlayerBinding {
        source_epoch: 3,
        lot_incarnation: 4,
        lot_location: 55,
        avatar_id: 7,
    };
    let mut authority = BuildAuthority::new(actor, PersistentId(7), 1000);
    authority.can_build = true;
    authority.prices.floor = 7;
    authority.permissions_revision = 11;
    authority.account_revision = 12;
    authority.catalog_revision = 13;
    let grant = ConstructionGrant {
        binding,
        principal: support::PRINCIPAL,
        authority,
        floor_patterns: BTreeSet::from([1, 2]),
    };
    let request = ConstructionRequest {
        binding,
        request_id: 9_007_199_254_740_993,
        lot_id: 11,
        authority_epoch: 7,
        architecture_revision: server.sim().state().world.lot.revision().architecture,
        permissions_revision: 11,
        account_revision: 12,
        catalog_revision: 13,
        selections: vec![Selection::Floor {
            tile: TilePos::new(2, 2, 1),
            pattern: 1,
        }],
    };
    let operation = u64::MAX - 14;
    let mut session = ConstructionSession::new(&server, &grant, 1000).unwrap();
    let mut client = Client::new(binding, 11, 7).unwrap();
    let before = server.snapshot().unwrap();
    let before_hash = hex(&server.sim().state_hash().unwrap());
    let quote_call = client.begin(request.clone()).unwrap();
    let offered = exchange::dispatch(
        &mut session,
        &mut server,
        &grant,
        &quote_call.bytes,
        Some(operation),
        100,
    )
    .unwrap();
    assert!(offered.admitted.is_none());
    assert_eq!(before, server.snapshot().unwrap());
    let quote_reply = offered.reply.unwrap();
    client.accept(quote_call.connection, &quote_reply).unwrap();
    assert_eq!(client.stage(), ClientStage::Reviewing);
    assert_eq!(client.quote().unwrap().consent.cost, 7);
    assert_eq!(client.quote().unwrap().operation, operation);
    let confirm = client.confirm(client.quote().unwrap().consent).unwrap();
    let result =
        exchange::dispatch(&mut session, &mut server, &grant, &confirm.bytes, None, 101).unwrap();
    let admitted = result.admitted.unwrap();
    let requests = admitted
        .outcome
        .events
        .iter()
        .filter(|event| matches!(event, RuntimeEvent::BuildRequested(_)))
        .count();
    assert_eq!(requests, 1);
    replica
        .apply_batch(replica.connection(), &[admitted.frame])
        .unwrap();
    let pending_hash = hex(&server.sim().state_hash().unwrap());
    let after_admission = server.snapshot().unwrap();
    assert_eq!(
        server
            .sim()
            .state()
            .world
            .lot
            .tile(TilePos::new(2, 2, 1))
            .unwrap()
            .floor,
        0
    );
    let repeated =
        exchange::dispatch(&mut session, &mut server, &grant, &confirm.bytes, None, 102).unwrap();
    assert!(repeated.admitted.is_none());
    assert_eq!(after_admission, server.snapshot().unwrap());
    // Lose the first reply; recovery may query but never automatically confirm again.
    client.disconnect().unwrap();
    session.disconnect();
    client.reconnect().unwrap();
    assert_eq!(client.stage(), ClientStage::Unknown);
    assert!(client.confirm(client.quote().unwrap().consent).is_err());
    let query = client.poll().unwrap();
    let query_value = exchange::decode_call(&query.bytes).unwrap();
    assert!(
        matches!(query_value.command,exchange::Command::Status {operation:Some(id),..} if id==operation)
    );
    let recovery =
        exchange::dispatch(&mut session, &mut server, &grant, &query.bytes, None, 103).unwrap();
    assert!(recovery.admitted.is_none());
    let recovery_reply = recovery.reply.unwrap();
    client.accept(query.connection, &recovery_reply).unwrap();
    assert_eq!(client.stage(), ClientStage::Pending);
    assert_eq!(server.snapshot().unwrap(), after_admission);
    let pending = server
        .sim()
        .state()
        .world
        .builds
        .pending_effect()
        .unwrap()
        .clone();
    let accepted = server
        .sim()
        .next_tick(vec![AcceptedCommand::CompleteBuild(
            ServerBuildConfirmation {
                operation: pending.operation,
                actor: pending.actor,
                owner: pending.owner,
                preview_hash: pending.preview_hash,
                charged_cost: pending.cost,
                durable_receipt: 99,
                status: DurableBuildStatus::Committed,
                created_objects: BTreeMap::new(),
            },
        )])
        .unwrap();
    let completion = server.apply_accepted(&accepted).unwrap();
    replica
        .apply_batch(
            replica.connection(),
            &[live_session::TickFrame {
                accepted,
                state_hash: completion.state_hash,
            }],
        )
        .unwrap();
    assert_eq!(
        replica.runtime().unwrap().snapshot().unwrap(),
        server.snapshot().unwrap()
    );
    assert!(
        client
            .observe(query.connection, &replica.projection().unwrap())
            .unwrap()
    );
    assert_eq!(client.stage(), ClientStage::Committed);
    assert!(
        !client
            .observe(query.connection, &replica.projection().unwrap())
            .unwrap()
    );
    let final_floor = server
        .sim()
        .state()
        .world
        .lot
        .tile(TilePos::new(2, 2, 1))
        .unwrap()
        .floor;
    assert_eq!(final_floor, 1);
    let encoded = json!({"schema":1,"fixture":"declared-source-construction","request_id":request.request_id.to_string(),
        "operation":operation.to_string(),"cost":7,"before_hash":before_hash,"pending_hash":pending_hash,
        "committed_hash":hex(&completion.state_hash),"replica_equal":true,"quoted_state_unchanged":true,
        "floor_before":0,"floor_pending":0,"floor_after":final_floor,"durable_requests":requests,
        "duplicate_admissions":0,"unknown_confirmation_blocked":true,"final_stage":format!("{:?}",client.stage()),
        "quote_call_hex":hex(&quote_call.bytes),"quote_reply_hex":hex(&quote_reply),
        "confirm_call_hex":hex(&confirm.bytes),"status_call_hex":hex(&query.bytes),"status_reply_hex":hex(&recovery_reply)});
    serde_json::to_vec(&encoded).unwrap()
}
static OUTPUT: OnceLock<Vec<u8>> = OnceLock::new();
// Probe-only exports of immutable, process-lifetime bytes; no caller pointer is read.
#[unsafe(no_mangle)]
pub extern "C" fn construction_probe_ptr() -> *const u8 {
    OUTPUT.get_or_init(run).as_ptr()
}
#[unsafe(no_mangle)]
pub extern "C" fn construction_probe_len() -> usize {
    OUTPUT.get_or_init(run).len()
}
fn main() {
    println!("{}", std::str::from_utf8(OUTPUT.get_or_init(run)).unwrap());
}
