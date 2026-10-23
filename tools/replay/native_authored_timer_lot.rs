//! Private pre-admission authored object setup. This is neither a purchase nor
//! a public object-creation command. The owner receipt must come from the trusted
//! runner's real AccountAuthority account/avatar creation and custody check.
#[path = "../../services/native-lot-host/src/loader.rs"]
mod loader;
const TIMER_GUID: u32 = 0x39D04C9C;
const TIMER_SOURCE_SHA256: &str =
    "6927573513f0ffdcc26879d8366242f8c26c03baf21080d1a7126386ff04a092";
use serde::Deserialize;
use serde_json::{Value, json};
use std::{fs, path::Path};
use wonderland_content_ir::manifest::Digest;
use wonderland_game_runtime::{
    AcceptedCommand, Facing, GameRuntime, PersistentId, RuntimeEvent, RuntimeRole, SimRuntime,
    SpawnSpec, TilePos,
    sim_core::{
        lot_permissions::{LotPermissionState, ReplicatedLotPermissions},
        owned_admission::OwnedAdmissionState,
    },
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const JOIN_ANCHOR_GUID: u32 = 0x1d95c9b0;
const VERIFIED_RELEASE: &str = "8921d56fa9261bdfc0f96c740eaa9a1ee240c60784916d5f704c3ef3fe5a5da4";
const VERIFIED_INTAKE: &str =
    include_str!("../../docs/verification/native-timer-delivery-20261015/source-release.json");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    schema_version: u32,
    base_manifest: String,
    base_manifest_sha256: String,
    base_host_config: String,
    base_host_config_sha256: String,
    account_creation_receipt: String,
    account_creation_receipt_sha256: String,
    original_timer_iff: String,
    placements: Vec<Placement>,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Placement {
    tile_x: i16,
    tile_y: i16,
    level: u8,
    facing: u8,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerReceipt {
    schema: String,
    authority_id: String,
    content_release: String,
    protocol_release: String,
    account_id: String,
    avatar_id: String,
    avatar_spec_sha256: String,
    source: String,
    creation_operation: String,
    canonical_create_result_sha256: String,
}
fn pinned(path: &str, expected: &str, limit: usize) -> Result<Vec<u8>> {
    if !Path::new(path).is_absolute() {
        return Err("input path must be absolute".into());
    }
    let bytes = loader::read(Path::new(path), limit)?;
    if Digest::of(&bytes).as_str() != expected {
        return Err("input digest mismatch".into());
    }
    Ok(bytes)
}
fn decimal(text: &str) -> Result<u64> {
    let n: u64 = text.parse()?;
    if n == 0 || n.to_string() != text {
        return Err("noncanonical positive identity".into());
    }
    Ok(n)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn safe_outcome(outcome: &wonderland_game_runtime::TickOutcome) -> Result<()> {
    if !outcome.effects.is_empty()
        || outcome.events.iter().any(|event| {
            matches!(
                event,
                RuntimeEvent::ThreadFault { .. } | RuntimeEvent::ExternalRequested { .. }
            )
        })
    {
        return Err(format!("original object lifecycle refused: {:?}", outcome.events).into());
    }
    Ok(())
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: native-authored-object-lot SETTINGS.json NEW_PRIVATE_DIR".into());
    }
    let settings_bytes = loader::read(Path::new(&args[0]), 65536)?;
    let settings: Settings = serde_json::from_slice(&settings_bytes)?;
    let output = Path::new(&args[1]);
    if settings.schema_version != 1
        || !output.is_absolute()
        || output.exists()
        || settings.placements.len() != 1
    {
        return Err("explicit new single-object authored Timer settings required".into());
    }
    if settings
        .placements
        .iter()
        .any(|p| p.level != 1 || p.facing > 7)
    {
        return Err("unsupported authored Timer layout".into());
    }
    let manifest_bytes = pinned(
        &settings.base_manifest,
        &settings.base_manifest_sha256,
        1024 * 1024,
    )?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes)?;
    let base_config_bytes = pinned(
        &settings.base_host_config,
        &settings.base_host_config_sha256,
        65536,
    )?;
    let mut config: Value = serde_json::from_slice(&base_config_bytes)?;
    if config["binding_sha256"].as_str() != Some(VERIFIED_RELEASE) {
        return Err("source-verified mailbox/Timer release required".into());
    }
    let receipt_bytes = pinned(
        &settings.account_creation_receipt,
        &settings.account_creation_receipt_sha256,
        65536,
    )?;
    let owner: OwnerReceipt = serde_json::from_slice(&receipt_bytes)?;
    if owner.schema != "wonderland.authored-object-owner-account.v1"
        || owner.source != "AccountAuthority.createAvatar"
        || owner.content_release
            != config["binding_sha256"]
                .as_str()
                .ok_or("base content pin missing")?
        || owner.creation_operation.len() < 16
        || owner.creation_operation.len() > 96
        || !owner
            .creation_operation
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err("unexpected actual account owner receipt".into());
    }
    for hash in [
        &owner.authority_id,
        &owner.content_release,
        &owner.protocol_release,
        &owner.avatar_spec_sha256,
        &owner.canonical_create_result_sha256,
    ] {
        Digest::try_from(hash.clone())?;
    }
    if manifest["authority_id"].as_str() != Some(&owner.authority_id)
        || manifest["content_release"].as_str() != Some(&owner.content_release)
        || manifest["protocol_release"].as_str() != Some(&owner.protocol_release)
        || manifest["catalog"]["release"].as_str() != Some(&owner.content_release)
        || manifest["host_config"].as_str() != Some(&settings.base_host_config)
        || manifest["lot"]["lot_id"] != config["lot_id"]
    {
        return Err("owner receipt does not belong to the pinned original manifest".into());
    }
    let verified: Value = serde_json::from_str(VERIFIED_INTAKE)?;
    if verified["binding_sha256"].as_str() != Some(VERIFIED_RELEASE)
        || verified["source_inputs"]["timer.iff"].as_str() != Some(TIMER_SOURCE_SHA256)
        || verified["timer_guid"].as_u64() != Some(u64::from(TIMER_GUID))
    {
        return Err("compiled original Timer source linkage is inconsistent".into());
    }
    decimal(&owner.account_id)?;
    let owner_id = decimal(&owner.avatar_id)?;
    if owner_id > 0x00ff_ffff {
        return Err("owner avatar exceeds source persistent identity range".into());
    }
    pinned(
        &settings.original_timer_iff,
        TIMER_SOURCE_SHA256,
        1024 * 1024,
    )?;
    let base_path = Path::new(&settings.base_host_config);
    let (mut game, runtime_config, _, load_report, takeover) = loader::load(base_path)?;
    if verified["runtime_content_sha256"] != load_report["runtime"]["content_sha256"]
        || verified["game_runtime_content_sha256"].as_str()
            != Some(&hex(&game.sim().state().content.content_hash))
    {
        return Err("loaded Timer runtime differs from source-verified content".into());
    }
    if takeover.is_some()
        || game.sim().state().completed_tick > 16
        || game.sim().state().entities.len() != 1
        || game.sim().state().entities.values().any(|entity| {
            entity.info.guid != JOIN_ANCHOR_GUID
                || entity.info.is_avatar
                || entity.info.persistent_id != 0
                || entity.info.position.x == i16::MIN
                || entity.disabled_flags != 0
        })
        || game
            .sim()
            .state()
            .owned_admission
            .actors()
            .is_none_or(|actors| !actors.is_empty())
        || game.sim().state().lot_permissions
            != ReplicatedLotPermissions::Initialized(LotPermissionState::default())
    {
        return Err(
            "authored setup requires original mailbox-only no-actor source-default genesis".into(),
        );
    }
    let anchor = game
        .sim()
        .state()
        .entities
        .values()
        .next()
        .ok_or("join anchor absent")?
        .info
        .reference;
    if game
        .sim()
        .state()
        .world
        .object(anchor)
        .and_then(|object| object.owner)
        != Some(PersistentId(0))
    {
        return Err("join anchor must retain original unowned source state".into());
    }
    let mut source = match &game.sim().state().owned_admission {
        OwnedAdmissionState::Initialized {
            source,
            revision: 1,
            permission_revision: 1,
            last_principal: 0,
            ..
        } if source.object_flags.len() == 1
            && source.object_flags.get(&anchor) == Some(&0)
            && source.registered_groups.is_empty() =>
        {
            source.as_ref().clone()
        }
        _ => return Err("base admission source is not pristine".into()),
    };
    let definition = game
        .sim()
        .content()
        .object(TIMER_GUID)
        .ok_or("cooked original Timer missing")?;
    if definition.definition.get(3) != Some(&100)
        || definition.entry_points.get(&0).map(|k| k.id) != Some(4096)
        || definition.entry_points.get(&1).map(|k| k.id) != Some(4097)
        || definition.placement_rules.is_avatar
        || definition.master_guid.is_some()
    {
        return Err("cooked Timer differs from required single-tile source definition".into());
    }
    let mut appearance = loader::appearance(base_path)?.ok_or("base appearance missing")?;
    if !appearance.objects.is_empty() || !appearance.models.is_empty() {
        return Err("base authored appearance must have no objects/models".into());
    }
    // Explicit source intake before any custody or coordinator exists. Runtime
    // state is revalidated at both boundaries; this is not a player tick/grant.
    let mut intake = game.sim().state().clone();
    intake.owned_admission = OwnedAdmissionState::Uninitialized;
    let mut sim =
        SimRuntime::from_state(intake, game.sim().content().clone(), RuntimeRole::Authority)?;
    let accepted = sim.next_tick(
        settings
            .placements
            .iter()
            .map(|p| {
                AcceptedCommand::Spawn(SpawnSpec {
                    guid: TIMER_GUID,
                    position: TilePos::new(p.tile_x, p.tile_y, p.level).center(),
                    facing: Facing(p.facing),
                    persistent_id: PersistentId(0),
                    avatar: false,
                })
            })
            .collect(),
    )?;
    safe_outcome(&sim.step(&accepted)?)?;
    for _ in 0..8 {
        let accepted = sim.next_tick(vec![])?;
        safe_outcome(&sim.step(&accepted)?)?;
    }
    if sim.state().entities.len() != 2 || !sim.pending_effects()?.is_empty() {
        return Err("original source lifecycle produced unexpected entities/effects".into());
    }
    let mut intake = sim.state().clone();
    let mut object_receipts = Vec::new();
    for entity in sim
        .state()
        .entities
        .values()
        .filter(|entity| entity.info.reference != anchor)
    {
        if entity.info.guid != TIMER_GUID
            || entity.info.persistent_id != 0
            || entity.info.is_avatar
            || entity.disabled_flags != 0
        {
            return Err(format!("original Timer lifecycle not established: {:?}", entity).into());
        }
        let mut placed = intake
            .world
            .object(entity.info.reference)
            .ok_or("placed object missing")?
            .clone();
        placed.owner = Some(PersistentId(owner_id as u32));
        intake
            .world
            .replace_object(placed)
            .map_err(|error| format!("source object ownership: {error:?}"))?;
        source.object_flags.insert(entity.info.reference, 0);
        object_receipts.push(json!({"entity":entity.info.reference,"guid":entity.info.guid,"position":entity.info.position,"facing":entity.info.direction,"persistent_id":0,"owner_avatar_id":owner.avatar_id,"attributes":entity.attributes,"graphic":entity.object_data[0]}));
    }
    let import_receipt = json!({"schema":"wonderland.authored-object-source-intake.v1","kind":"explicit private authored setup, not lot purchase, roommate grant or user object-creation flow","settings_sha256":Digest::of(&settings_bytes),"base_manifest_sha256":Digest::of(&manifest_bytes),"base_host_config_sha256":settings.base_host_config_sha256,"base_checkpoint_sha256":config["recovery"]["checkpoint_sha256"],"verified_release_binding_sha256":VERIFIED_RELEASE,"verified_release_intake_sha256":Digest::of(VERIFIED_INTAKE.as_bytes()),"preserved_join_anchor":anchor,"account_creation_receipt_sha256":settings.account_creation_receipt_sha256,"authority_id":owner.authority_id,"owner_account_id":owner.account_id,"owner_avatar_id":owner.avatar_id,"placements":settings.placements,"objects":object_receipts,"source_visuals":"not imported; headless source qualification only","lot_permissions":"source_default_unowned","avatar_roles_changed":false,"database_object_ids_allocated":false,"registered_persistent_groups":0});
    let import_bytes = serde_json::to_vec(&import_receipt)?;
    let import_hash = Digest::of(&import_bytes);
    let mut import_digest = [0; 32];
    for (i, pair) in import_hash.as_str().as_bytes().chunks_exact(2).enumerate() {
        import_digest[i] = u8::from_str_radix(std::str::from_utf8(pair)?, 16)?;
    }
    source.source_import_sha256 = import_digest;
    let mut sim = SimRuntime::from_state(intake, sim.content().clone(), RuntimeRole::Authority)?;
    let accepted = sim.next_tick(vec![AcceptedCommand::InitializeOwnedAdmission { source }])?;
    safe_outcome(&sim.step(&accepted)?)?;
    game = GameRuntime::new(
        sim.content().clone(),
        sim.state().world.lot.clone(),
        runtime_config,
        RuntimeRole::Authority,
    )?;
    game.restore(&sim.snapshot()?)?;
    // No substitute artwork or model is introduced by headless qualification.
    appearance.provenance.origin = "operator-authored-original-timer-layout".into();
    appearance.source_counts = None;
    appearance.validate()?;
    let world = serde_json::to_vec(&appearance)?;
    let checkpoint = game.snapshot()?;
    if world.len() > 32 * 1024 * 1024 || checkpoint.len() > 8 * 1024 * 1024 {
        return Err("authored outputs exceed size bounds".into());
    }
    let state = game.sim().state();
    let cursor = json!({"lot_id":state.lot_id.to_string(),"authority_epoch":state.authority_epoch.to_string(),"completed_tick":state.completed_tick.to_string(),"state_hash":hex(&game.sim().state_hash()?)});
    let path = |name: &str| output.join(name).to_string_lossy().into_owned();
    config["appearance_file"] = json!(path("world.json"));
    config["appearance_sha256"] = json!(Digest::of(&world));
    config["recovery"] = json!({"checkpoint_file":path("checkpoint.bin"),"checkpoint_sha256":Digest::of(&checkpoint),"tail_file":null,"tail_sha256":null,"additional_tail_files":[],"expected_cursor":cursor,"takeover":null});
    let host = serde_json::to_vec_pretty(&config)?;
    let result = json!({"schema":"wonderland.authored-object-ownership-receipt.v1","source_intake_sha256":import_hash,"account_creation_receipt_sha256":Digest::of(&receipt_bytes),"settings_sha256":Digest::of(&settings_bytes),"base_manifest_sha256":Digest::of(&manifest_bytes),"base_host_config_sha256":Digest::of(&base_config_bytes),"authority_id":owner.authority_id,"content_release":owner.content_release,"protocol_release":owner.protocol_release,"lot_id":manifest["lot"]["lot_id"],"host_config":path("host-config.json"),"host_config_sha256":Digest::of(&host),"checkpoint_sha256":Digest::of(&checkpoint),"appearance_sha256":Digest::of(&world),"cursor":cursor,"entities":2,"rendered_timer_objects":0,"unrendered_original_join_anchors":1,"owned_actors":0,"avatar_roles_changed":false,"owner_account_id":owner.account_id,"owner_avatar_id":owner.avatar_id,"source_intake":import_receipt});
    fs::create_dir(output)?;
    for (name, bytes) in [
        ("settings.json", settings_bytes),
        ("account-creation-receipt.json", receipt_bytes),
        ("source-intake.json", import_bytes),
        ("world.json", world),
        ("checkpoint.bin", checkpoint),
        ("host-config.json", host),
        (
            "object-ownership-receipt.json",
            serde_json::to_vec_pretty(&result)?,
        ),
    ] {
        fs::write(output.join(name), bytes)?;
    }
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("authored original object setup refused: {error}");
        std::process::exit(1);
    }
}
