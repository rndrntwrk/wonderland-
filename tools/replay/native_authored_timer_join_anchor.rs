//! Explicit private source intake for the original join mailbox. This creates
//! no avatar, role, purchase or fake visible replacement. Original Init/Main run.
#[path = "../../services/native-lot-host/src/loader.rs"]
mod loader;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{fs, path::Path};
use wonderland_content_ir::manifest::Digest;
use wonderland_game_runtime::{
    AcceptedCommand, Facing, PersistentId, RuntimeEvent, RuntimeRole, SimRuntime, SpawnSpec,
    TilePos,
    sim_core::{
        lot_permissions::{LotPermissionState, ReplicatedLotPermissions},
        owned_admission::OwnedAdmissionState,
    },
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const GUID: u32 = 0x1d95c9b0;
// This versioned intake accepts only the sealed release whose original source
// linkage was independently checked during the private cooker qualification.
const VERIFIED_RELEASE: &str = "8921d56fa9261bdfc0f96c740eaa9a1ee240c60784916d5f704c3ef3fe5a5da4";
const VERIFIED_INTAKE: &str =
    include_str!("../../docs/verification/native-timer-delivery-20261015/source-release.json");
const SOURCE_SHA256: &str = "f00948388f143c22c08facd2bdf73896d44e41a1de6f223155acea6d40382de9";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    schema_version: u32,
    base_host_config: String,
    base_host_config_sha256: String,
    original_mailbox_iff: String,
    tile_x: i16,
    tile_y: i16,
    facing: u8,
}
fn pinned(path: &str, hash: &str, limit: usize) -> Result<Vec<u8>> {
    if !Path::new(path).is_absolute() {
        return Err("absolute input path required".into());
    }
    let bytes = loader::read(Path::new(path), limit)?;
    if Digest::of(&bytes).as_str() != hash {
        return Err("input pin mismatch".into());
    }
    Ok(bytes)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn digest_bytes(bytes: &[u8]) -> [u8; 32] {
    let digest = Digest::of(bytes);
    let mut result = [0; 32];
    for (out, pair) in result
        .iter_mut()
        .zip(digest.as_str().as_bytes().chunks_exact(2))
    {
        *out = u8::from_str_radix(std::str::from_utf8(pair).expect("digest ASCII"), 16)
            .expect("digest hex");
    }
    result
}
fn safe(outcome: &wonderland_game_runtime::TickOutcome) -> Result<()> {
    if !outcome.effects.is_empty()
        || outcome.events.iter().any(|event| {
            matches!(
                event,
                RuntimeEvent::ThreadFault { .. } | RuntimeEvent::ExternalRequested { .. }
            )
        })
    {
        return Err(format!("original mailbox lifecycle refused: {:?}", outcome.events).into());
    }
    Ok(())
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: native-authored-join-anchor SETTINGS.json NEW_PRIVATE_DIR".into());
    }
    let settings_bytes = loader::read(Path::new(&args[0]), 65536)?;
    let settings: Settings = serde_json::from_slice(&settings_bytes)?;
    let output = Path::new(&args[1]);
    if settings.schema_version != 1
        || !output.is_absolute()
        || output.exists()
        || settings.facing > 7
    {
        return Err("invalid explicit authored join anchor settings".into());
    }
    let config_bytes = pinned(
        &settings.base_host_config,
        &settings.base_host_config_sha256,
        65536,
    )?;
    let mut config: Value = serde_json::from_slice(&config_bytes)?;
    if config["binding_sha256"].as_str() != Some(VERIFIED_RELEASE) {
        return Err(
            "mailbox source intake requires the exact source-verified sealed release".into(),
        );
    }
    let verified: Value = serde_json::from_str(VERIFIED_INTAKE)?;
    if verified["binding_sha256"].as_str() != Some(VERIFIED_RELEASE)
        || verified["source_inputs"]["mailboxvacation.iff"].as_str() != Some(SOURCE_SHA256)
        || verified["mailbox_guid"].as_u64() != Some(u64::from(GUID))
    {
        return Err("compiled original mailbox release linkage is inconsistent".into());
    }
    pinned(&settings.original_mailbox_iff, SOURCE_SHA256, 65536)?;
    let base_path = Path::new(&settings.base_host_config);
    let (game, _, _, _, takeover) = loader::load(base_path)?;
    if takeover.is_some()
        || !game.sim().state().entities.is_empty()
        || !game.sim().state().threads.is_empty()
        || game.sim().state().completed_tick > 16
        || game.sim().state().lot_permissions
            != ReplicatedLotPermissions::Initialized(LotPermissionState::default())
    {
        return Err("anchor intake requires pristine empty unowned genesis".into());
    }
    let definition = game
        .sim()
        .content()
        .object(GUID)
        .ok_or("sealed original vacation mailbox missing")?;
    if definition.master_guid.is_some()
        || definition.placement_rules.is_avatar
        || definition.definition.get(3) != Some(&100)
        || definition.definition.get(4) != Some(&1)
        || definition.entry_points.get(&0).map(|key| key.id) != Some(4101)
        || definition.entry_points.get(&1).map(|key| key.id) != Some(4096)
    {
        return Err("sealed object is not original single-tile vacation mailbox".into());
    }
    let mut source = match &game.sim().state().owned_admission {
        OwnedAdmissionState::Initialized {
            source,
            revision: 1,
            permission_revision: 1,
            last_principal: 0,
            actors,
        } if actors.is_empty()
            && source.object_flags.is_empty()
            && source.registered_groups.is_empty() =>
        {
            source.as_ref().clone()
        }
        _ => return Err("source admission is not pristine".into()),
    };
    let appearance = loader::appearance(base_path)?.ok_or("base appearance missing")?;
    if !appearance.objects.is_empty() || !appearance.models.is_empty() {
        return Err("base appearance is not empty".into());
    }
    let mut intake = game.sim().state().clone();
    intake.owned_admission = OwnedAdmissionState::Uninitialized;
    let mut sim =
        SimRuntime::from_state(intake, game.sim().content().clone(), RuntimeRole::Authority)?;
    let tick = sim.next_tick(vec![AcceptedCommand::Spawn(SpawnSpec {
        guid: GUID,
        position: TilePos::new(settings.tile_x, settings.tile_y, 1).center(),
        facing: Facing(settings.facing),
        persistent_id: PersistentId(0),
        avatar: false,
    })])?;
    safe(&sim.step(&tick)?)?;
    let tick = sim.next_tick(vec![])?;
    safe(&sim.step(&tick)?)?;
    if sim.state().entities.len() != 1 || !sim.pending_effects()?.is_empty() {
        return Err("unexpected mailbox lifecycle entities/effects".into());
    }
    let entity = sim
        .state()
        .entities
        .values()
        .next()
        .ok_or("mailbox missing")?;
    if entity.info.guid != GUID
        || entity.info.is_avatar
        || entity.info.persistent_id != 0
        || entity.info.position.x == i16::MIN
        || entity.disabled_flags != 0
    {
        return Err("original mailbox did not become an ordinary placed source object".into());
    }
    let reference = entity.info.reference;
    let mut intake = sim.state().clone();
    let mut placed = intake
        .world
        .object(reference)
        .ok_or("mailbox world object missing")?
        .clone();
    // VMTSOObjectState constructor owner default. No account owns this anchor.
    placed.owner = Some(PersistentId(0));
    intake
        .world
        .replace_object(placed)
        .map_err(|e| format!("source world ownership: {e:?}"))?;
    source.object_flags.insert(reference, 0);
    let source_receipt = json!({"schema":"wonderland.authored-original-join-anchor.v1","kind":"explicit authored original source object placement, not an original shipped layout","base_host_config_sha256":Digest::of(&config_bytes),"base_checkpoint_sha256":config["recovery"]["checkpoint_sha256"],"settings_sha256":Digest::of(&settings_bytes),"source_iff_sha256":SOURCE_SHA256,"verified_release_binding_sha256":VERIFIED_RELEASE,"verified_release_intake_sha256":Digest::of(VERIFIED_INTAKE.as_bytes()),"source_path":"objectdata/objects/objiff.far!mailboxvacation.iff","guid":GUID,"entity":reference,"position":entity.info.position,"facing":entity.info.direction,"owner_id":0,"persistent_id":0,"source_default_object_flags":0,"registered_persistent_groups":0,"initialization":"original OBJf init4101/main4096 with original global271/280","avatar_created":false,"avatar_flags_or_positions_modified":false,"visual_status":"original mailbox SPF is absent; no replacement model supplied"});
    let receipt_bytes = serde_json::to_vec(&source_receipt)?;
    source.source_import_sha256 = digest_bytes(&receipt_bytes);
    let mut sim = SimRuntime::from_state(intake, sim.content().clone(), RuntimeRole::Authority)?;
    let tick = sim.next_tick(vec![AcceptedCommand::InitializeOwnedAdmission { source }])?;
    safe(&sim.step(&tick)?)?;
    let checkpoint = sim.snapshot()?;
    if checkpoint.len() > 8 * 1024 * 1024 {
        return Err("checkpoint bound".into());
    }
    let state = sim.state();
    let cursor = json!({"lot_id":state.lot_id.to_string(),"authority_epoch":state.authority_epoch.to_string(),"completed_tick":state.completed_tick.to_string(),"state_hash":hex(&sim.state_hash()?)});
    let path = |name: &str| output.join(name).to_string_lossy().into_owned();
    config["recovery"] = json!({"checkpoint_file":path("checkpoint.bin"),"checkpoint_sha256":Digest::of(&checkpoint),"tail_file":null,"tail_sha256":null,"additional_tail_files":[],"expected_cursor":cursor,"takeover":null});
    let host = serde_json::to_vec_pretty(&config)?;
    let result = json!({"schema":"wonderland.authored-original-join-anchor-result.v1","host_config":path("host-config.json"),"host_config_sha256":Digest::of(&host),"checkpoint_sha256":Digest::of(&checkpoint),"source_receipt_sha256":Digest::of(&receipt_bytes),"cursor":cursor,"entities":1,"owned_actors":0,"source":source_receipt,"two_avatar_admission_executed":false});
    fs::create_dir(output)?;
    for (name, bytes) in [
        ("settings.json", settings_bytes),
        ("source-receipt.json", receipt_bytes),
        ("checkpoint.bin", checkpoint),
        ("host-config.json", host),
        ("result.json", serde_json::to_vec_pretty(&result)?),
    ] {
        fs::write(output.join(name), bytes)?;
    }
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("authored original join anchor refused: {error}");
        std::process::exit(1)
    }
}
