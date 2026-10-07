//! Portable conformance for the real native wire + retained Vitaboy presentation.
//! The tiny geometry/content metadata below are explicitly synthetic fixtures.
#[path = "../tests/support/native_avatar_bank.rs"]
mod bank;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::{Arc, OnceLock};
use wonderland_game_runtime::live_session::{Checkpoint, TickFrame};
use wonderland_game_runtime::live_wire::{
    WireLimits, encode_checkpoint, encode_ticks,
    player::{Bootstrap, NativePlayer, PlayerBinding, PlayerUpdate, encode_bootstrap},
};
use wonderland_game_runtime::sim_core::{
    avatars::{
        outfits::OutfitReference,
        timeline::{AnimationMetadata, AnimationState},
    },
    state::{AnimationKey, ContentSet, ObjectDefinition, TuningSet},
    vm::RoutineStore,
};
use wonderland_game_runtime::{
    AcceptedCommand, Facing, GameRuntime, LotModel, PersistentId, RuntimeConfig, RuntimeEvent,
    RuntimeRole, SpawnSpec, TilePos, VmMode,
};
// Compile the exact DOM-free production source, not the complete web-shell
// library: its unrelated wasm-bindgen browser exports require JavaScript glue.
#[allow(dead_code)]
#[path = "../src/native_avatar.rs"]
mod native_avatar;
use native_avatar::{NativeAvatarPoseHistory, NativeAvatarProjection};
use wonderland_world_view::WorldDocument;
const ADULT: u32 = 0x7FD96B54;
fn hash<T: serde::Serialize>(value: &T) -> String {
    hex(&Sha256::digest(serde_json::to_vec(value).unwrap()))
}
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for b in bytes {
        write!(out, "{b:02x}").unwrap();
    }
    out
}
fn setup() -> (GameRuntime, NativePlayer) {
    let animation = AnimationMetadata {
        resource: "base.anim".into(),
        num_frames: 2,
        time_properties: vec![],
    };
    let content = ContentSet::new(
        RoutineStore::new(),
        vec![ObjectDefinition::new(ADULT, 0)],
        vec![(
            AnimationKey {
                owner: ADULT,
                scope: 0,
                id: 1,
            },
            animation.clone(),
        )],
        TuningSet::default(),
    )
    .unwrap();
    let mut server = GameRuntime::new(
        content,
        LotModel::new(8, 8, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 11, 7, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let spawned = server
        .advance(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: ADULT,
            position: TilePos::new(3, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(42),
            avatar: true,
        })])
        .unwrap();
    let actor = spawned
        .events
        .iter()
        .find_map(|event| {
            if let RuntimeEvent::Spawned(actor) = event {
                Some(*actor)
            } else {
                None
            }
        })
        .unwrap();
    let mut state = server.sim().state().clone();
    let avatar = state
        .entities
        .get_mut(&actor.object_id)
        .unwrap()
        .avatar
        .as_mut()
        .unwrap();
    avatar.outfits.head = Some(OutfitReference::Id(bank::HEAD));
    avatar.outfits.body = Some(OutfitReference::Id(bank::BODY));
    let mut animation = AnimationState::new(animation, false).unwrap();
    animation.speed = 0.25;
    avatar.animations.animations.push(animation);
    let bytes = wonderland_game_runtime::sim_core::snapshot::encode(&state, server.sim().content())
        .unwrap();
    server.restore(&bytes).unwrap();
    let state = server.sim().state();
    let binding = PlayerBinding {
        source_epoch: 3,
        lot_incarnation: 4,
        lot_location: 55,
        avatar_id: 42,
    };
    let bootstrap = Bootstrap {
        binding,
        principal: wonderland_game_runtime::PrincipalKey(42),
        actor,
        content: server.sim().content().clone(),
        appearance: WorldDocument::from_blueprint_xml(
            "<house><size>8</size><world><floors/><walls/></world><objects/></house>",
            "test:pose-history",
            "synthetic fixture",
        )
        .unwrap(),
        lot: state.world.lot.clone(),
        mode: state.mode,
        lot_id: state.lot_id,
        authority_epoch: state.authority_epoch,
        effect_namespace: state.effects.namespace(),
        limits: state.limits.clone(),
        effect_limits: state.effects.limits(),
    };
    (
        server,
        NativePlayer::open(&encode_bootstrap(&bootstrap).unwrap(), binding, 2).unwrap(),
    )
}
fn install(
    p: &mut NativePlayer,
    server: &GameRuntime,
    history: &mut NativeAvatarPoseHistory,
    bank: &Arc<wonderland_avatar_content::ImportedContent>,
) {
    let bytes = server.snapshot().unwrap();
    let packet = encode_checkpoint(
        p.checkpoint_request().unwrap().id,
        Checkpoint {
            completed_tick: server.sim().state().completed_tick,
            state_hash: server.sim().state_hash().unwrap(),
            bytes: &bytes,
        },
        &[],
        WireLimits::default(),
    )
    .unwrap();
    let (update, frames) = p.receive_presented(&packet).unwrap();
    assert!(matches!(update, PlayerUpdate::Checkpoint(_)));
    history.clear();
    for frame in frames.unwrap() {
        history.observe(&frame, Arc::clone(bank)).unwrap();
    }
}
fn picture(
    p: &NativePlayer,
    history: &NativeAvatarPoseHistory,
    bank: &Arc<wonderland_avatar_content::ImportedContent>,
) -> WorldDocument {
    let mut world = p.world().unwrap();
    let frame = p.avatar_visual_frame().unwrap();
    NativeAvatarProjection::prepare_retained(&frame, &world, bank, history)
        .unwrap()
        .apply(&mut world, bank, &bank::pixels(bank))
        .unwrap();
    assert_eq!(world.models.len(), 1);
    world
}
fn run(batch_size: usize) -> Value {
    let (mut server, mut p) = setup();
    let bank = Arc::new(bank::bank());
    let mut history = NativeAvatarPoseHistory::default();
    install(&mut p, &server, &mut history, &bank);
    let initial = picture(&p, &history, &bank);
    let mut frames = Vec::new();
    let mut trace = Vec::new();
    for _ in 0..18 {
        let accepted = server.sim().next_tick(vec![]).unwrap();
        let result = server.apply_accepted(&accepted).unwrap();
        frames.push(TickFrame {
            accepted,
            state_hash: result.state_hash,
        });
    }
    for batch in frames.chunks(batch_size) {
        let (update, visuals) = p
            .receive_presented(&encode_ticks(batch, WireLimits::default()).unwrap())
            .unwrap();
        assert!(matches!(update,PlayerUpdate::Ticks(ref o) if o.len()==batch.len()));
        for frame in visuals.unwrap() {
            trace.push(json!({"tick":frame.revision.tick,"avatars":frame.avatars}));
            history.observe(&frame, Arc::clone(&bank)).unwrap();
        }
        let _ = picture(&p, &history, &bank);
    }
    let final_world = picture(&p, &history, &bank);
    assert_ne!(
        initial.models[0].groups, final_world.models[0].groups,
        "an ended clip must retain its last accepted translation"
    );
    let (_, duplicate) = p
        .receive_presented(&encode_ticks(&frames[17..], WireLimits::default()).unwrap())
        .unwrap();
    assert!(duplicate.unwrap().is_empty());
    let model = hash(&final_world.models[0]);
    for _ in 0..120 {
        assert_eq!(hash(&picture(&p, &history, &bank).models[0]), model);
    }
    let server_hash = hex(&server.sim().state_hash().unwrap());
    p.disconnect();
    assert!(p.avatar_visual_frame().is_err());
    p.reconnect().unwrap();
    install(&mut p, &server, &mut history, &bank);
    let cold = picture(&p, &history, &bank);
    assert_eq!(
        cold.models[0].groups, initial.models[0].groups,
        "unknown pre-checkpoint bone history is not fabricated"
    );
    json!({"trace":trace,"retained_model":final_world.models[0],"retained_sha256":model,"checkpoint_reset_model":cold.models[0],"server_hash":server_hash,"tick":server.sim().state().completed_tick,"duplicate_frames":0,"repeat_draws":120})
}
fn output() -> Vec<u8> {
    let ordinary = run(1);
    assert_eq!(ordinary, run(3));
    assert_eq!(ordinary, run(18));
    serde_json::to_vec(&json!({"schema":1,"scope":"synthetic original-format geometry with actual native accepted ticks","equal_batch_sizes":[1,3,18],"result":ordinary})).unwrap()
}
static OUTPUT: OnceLock<Vec<u8>> = OnceLock::new();
// SAFETY: probe-only exports of an immutable process-lifetime buffer; no host
// pointers are accepted or dereferenced and no production symbol is replaced.
#[unsafe(no_mangle)]
pub extern "C" fn wonderland_pose_probe_ptr() -> *const u8 {
    OUTPUT.get_or_init(output).as_ptr()
}
#[unsafe(no_mangle)]
pub extern "C" fn wonderland_pose_probe_len() -> usize {
    OUTPUT.get_or_init(output).len()
}
fn main() {
    println!(
        "{}",
        std::str::from_utf8(OUTPUT.get_or_init(output)).unwrap()
    );
}
