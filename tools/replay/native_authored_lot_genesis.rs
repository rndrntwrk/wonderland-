//! Private operator-driven genesis for a NEW authored empty lot.
//! Uses the shipping sealed loader, runtime and accepted initialization commands.
//! Creates no avatar, catalogue selection, credential, grant or original layout.
use bincode::Options;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest as _, Sha256};
use std::{collections::BTreeMap, fs, io::Read, path::Path};
use wonderland_content_ir::manifest::{AssetManifest, Digest, Origin};
use wonderland_game_runtime::{
    AcceptedCommand, GameRuntime, LotModel, RuntimeConfig, RuntimeRole, VmMode,
    content_bridge::cooked::{CookedLoadLimits, PackBytes, PreparedRelease, SOURCE_BASELINE},
    sim_core::{
        avatars::state::TEMPLATE_PERSON, lot_permissions::LotPermissionState,
        owned_admission::SourceJoinMetadata,
    },
    world_view::{WorldDocument, WorldSourceKind},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    schema_version: u32,
    release_dir: String,
    binding_sha256: String,
    local_identity_namespace: String,
    local_protocol_tag: String,
    lot_name: String,
    size: u16,
    seed: String,
    utc_start_dotnet_ticks: String,
    authority_epoch: String,
    lot_incarnation: String,
    permission_policy: String,
    dynamic_tuning_confirmed_absent: bool,
    avatar_guid: u32,
}
fn read(path: &Path, cap: usize) -> Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > cap as u64 {
        return Err("input file type or byte bound".into());
    }
    let mut bytes = Vec::new();
    file.take(cap as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > cap {
        return Err("input byte bound".into());
    }
    Ok(bytes)
}
fn decimal(text: &str, allow_zero: bool) -> Result<u64> {
    let n: u64 = text.parse()?;
    if n.to_string() != text || (!allow_zero && n == 0) {
        return Err("noncanonical operator identity".into());
    }
    Ok(n)
}
fn hash(domain: &str, bytes: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(domain.as_bytes());
    h.update([0]);
    h.update(bytes);
    h.finalize().into()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn positive32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes[..4].try_into().expect("four bytes")).max(1)
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err(
            "usage: native-authored-lot-genesis OPERATOR_SETTINGS.json NEW_PRIVATE_OUTPUT_DIR"
                .into(),
        );
    }
    let settings_bytes = read(Path::new(&args[0]), 65536)?;
    let s: Settings = serde_json::from_slice(&settings_bytes)?;
    let output = Path::new(&args[1]);
    if s.schema_version != 1
        || !(8..=64).contains(&s.size)
        || s.lot_name.is_empty()
        || s.lot_name.len() > 96
        || s.local_identity_namespace.is_empty()
        || s.local_identity_namespace.len() > 128
        || s.local_protocol_tag.is_empty()
        || s.local_protocol_tag.len() > 128
        || s.permission_policy != "source_default_unowned"
        || !s.dynamic_tuning_confirmed_absent
        || s.avatar_guid != TEMPLATE_PERSON
        || !Path::new(&s.release_dir).is_absolute()
        || !output.is_absolute()
        || output.exists()
    {
        return Err("unsupported or incomplete explicit genesis settings".into());
    }
    let seed = decimal(&s.seed, true)?;
    let utc = i64::try_from(decimal(&s.utc_start_dotnet_ticks, true)?)?;
    let epoch = decimal(&s.authority_epoch, false)?;
    let incarnation = decimal(&s.lot_incarnation, false)?;
    let canonical_settings = serde_json::to_vec(&s)?;
    let identity = hash(
        "wonderland.authored-local-lot-identities.v1",
        &canonical_settings,
    );
    let lot_id = positive32(&identity[..4]);
    let neighborhood = positive32(&identity[4..8]);
    let x = u32::from(u16::from_le_bytes(identity[8..10].try_into()?)) % 511 + 1;
    let y = u32::from(u16::from_le_bytes(identity[10..12].try_into()?)) % 511 + 1;
    let location = (x << 16) | y;
    let effect_namespace = u64::from_le_bytes(identity[12..20].try_into()?).max(1);
    let authority_id = hex(&hash(
        "wonderland.local-account-authority.v1",
        &canonical_settings,
    ));
    let protocol_release = hex(&hash(
        "wonderland.local-protocol-release.v1",
        &canonical_settings,
    ));
    let root = Path::new(&s.release_dir);
    let limits = CookedLoadLimits::default();
    let binding = read(&root.join("binding.json"), limits.max_binding_bytes)?;
    let manifest = read(
        &root.join("manifest.json"),
        limits.manifest.max_manifest_bytes,
    )?;
    let expected = Digest::try_from(s.binding_sha256.clone())?;
    let prepared = PreparedRelease::from_binding(&binding, &expected, &manifest, &limits)?;
    let total = prepared.required_packs().iter().try_fold(0u64, |n, p| {
        n.checked_add(p.byte_len).ok_or("pack total overflow")
    })?;
    if total > limits.max_total_pack_bytes {
        return Err("pack total bound".into());
    }
    let payloads: Vec<_> = prepared
        .required_packs()
        .iter()
        .map(|p| {
            read(
                &root.join(format!("{}.wlp", p.digest.as_str())),
                usize::try_from(p.byte_len)?,
            )
        })
        .collect::<Result<_>>()?;
    let packs: Vec<_> = prepared
        .required_packs()
        .iter()
        .zip(&payloads)
        .map(|(p, b)| PackBytes {
            digest: &p.digest,
            bytes: b,
        })
        .collect();
    let loaded = prepared.load(&packs, &limits)?;
    let verified_manifest = AssetManifest::from_json(&manifest, &limits.manifest)?;
    let avatar_report = loaded
        .report
        .objects
        .iter()
        .find(|o| o.guid == s.avatar_guid)
        .ok_or("avatar report missing")?;
    let origin = verified_manifest
        .resources
        .get(&avatar_report.object_resource)
        .ok_or("avatar provenance missing")?
        .provenance
        .origin;
    if !matches!(origin, Origin::Original | Origin::UserImported) {
        return Err("avatar archetype must retain original or user-imported provenance".into());
    }
    let original = loaded
        .content
        .object(s.avatar_guid)
        .ok_or("original avatar archetype missing")?;
    // Runtime definition 0/1 are version words; source OBJD ObjectType is word7.
    if original.definition.get(9) != Some(&2) || !original.placement_rules.is_avatar {
        return Err("verified OBJD is not a source person".into());
    }
    let lot = LotModel::new(s.size, s.size, 5).map_err(|e| format!("lot geometry: {e:?}"))?;
    let lot_bytes = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .serialize(&lot)?;
    if lot_bytes.len() > 16 * 1024 * 1024 {
        return Err("lot output bound".into());
    }
    let xml = format!(
        "<house><size>{}</size><category>0</category><world><floors/><walls/><pools/></world><objects/><sounds/></house>",
        s.size
    );
    let mut appearance = WorldDocument::from_blueprint_xml(
        &xml,
        "operator-authored-empty-lot.xml",
        SOURCE_BASELINE,
    )?;
    appearance.provenance.kind = WorldSourceKind::SavedWorld;
    appearance.validate()?;
    let appearance_bytes = serde_json::to_vec(&appearance)?;
    if appearance_bytes.len() > 32 * 1024 * 1024 {
        return Err("appearance output bound".into());
    }
    let mut config = RuntimeConfig::new(VmMode::Tso, u64::from(lot_id), epoch, seed);
    config.utc_start_dotnet_ticks = utc;
    config.effect_namespace = effect_namespace;
    let mut game = GameRuntime::from_cooked(loaded, lot, config.clone(), RuntimeRole::Authority)?;
    if !game.sim().state().entities.is_empty() || !game.sim().state().threads.is_empty() {
        return Err("genesis must start with no entities or threads".into());
    }
    let permissions = LotPermissionState::default();
    let input_receipt = json!({"schema":"wonderland.authored-lot-genesis-input.v1","kind":"new operator-authored empty lot; not an original shipped layout","operator_settings":s,"operator_settings_sha256":Digest::of(&settings_bytes),"canonical_settings_sha256":Digest::of(&canonical_settings),"binding_sha256":expected,"manifest_sha256":Digest::of(&manifest),"runtime_content":game.sim().state().content,"lot_sha256":Digest::of(&lot_bytes),"appearance_sha256":Digest::of(&appearance_bytes),"blueprint_sha256":Digest::of(xml.as_bytes()),"derived_local_only_identities":{"lot_id":lot_id.to_string(),"lot_location":location,"neighborhood_id":neighborhood,"effect_namespace":effect_namespace.to_string(),"authority_id":authority_id,"protocol_release":protocol_release},"permission_source":"VMTSOLotState constructor defaults","permissions":permissions,"dynamic_tuning":"explicitly absent; therefore aprilfools(0,2019) comparison is false","verified_original_avatar_guid":s.avatar_guid,"empty_entities_verified":true,"empty_threads_verified":true,"avatar_created":false});
    let receipt_bytes = serde_json::to_vec(&input_receipt)?;
    let receipt_hash: [u8; 32] = Sha256::digest(&receipt_bytes).into();
    let source = SourceJoinMetadata {
        source_import_sha256: receipt_hash,
        content: game.sim().state().content,
        neighborhood_id: neighborhood,
        lot_location: location,
        avatar_guids: [s.avatar_guid].into(),
        april_fools_2019: false,
        object_flags: BTreeMap::new(),
        registered_groups: BTreeMap::new(),
    };
    let outcome = game.advance(vec![
        AcceptedCommand::InitializeLotPermissions { lot: permissions },
        AcceptedCommand::InitializeOwnedAdmission { source },
    ])?;
    if !outcome.effects.is_empty()
        || !game.sim().state().entities.is_empty()
        || game
            .sim()
            .state()
            .owned_admission
            .actors()
            .is_none_or(|a| !a.is_empty())
    {
        return Err(
            "genesis unexpectedly created actors/effects or missed admission initialization".into(),
        );
    }
    game.sim()
        .state()
        .owned_admission
        .validate(game.sim().state())?;
    // Exercise matching world projection before publishing an appearance pin.
    game.world_document(&appearance)?;
    let checkpoint = game.snapshot()?;
    if checkpoint.len() > 8 * 1024 * 1024 {
        return Err("checkpoint output bound".into());
    }
    let state = game.sim().state();
    let cursor = json!({"lot_id":state.lot_id.to_string(),"authority_epoch":state.authority_epoch.to_string(),"completed_tick":state.completed_tick.to_string(),"state_hash":hex(&game.sim().state_hash()?)});
    let path = |name: &str| output.join(name).to_string_lossy().into_owned();
    let host = json!({"schema_version":1,"release_dir":s.release_dir,"binding_sha256":s.binding_sha256,"lot_file":path("lot.bin"),"lot_sha256":Digest::of(&lot_bytes),"mode":"Tso","lot_id":lot_id.to_string(),"authority_epoch":epoch.to_string(),"lot_incarnation":incarnation.to_string(),"seed":seed.to_string(),"utc_start_dotnet_ticks":utc.to_string(),"effect_namespace":effect_namespace.to_string(),"appearance_file":path("world.json"),"appearance_sha256":Digest::of(&appearance_bytes),"recovery":{"checkpoint_file":path("checkpoint.bin"),"checkpoint_sha256":Digest::of(&checkpoint),"tail_file":null,"tail_sha256":null,"additional_tail_files":[],"expected_cursor":cursor,"takeover":null}});
    let pending = json!({"authority_id":authority_id,"content_release":s.binding_sha256,"protocol_release":protocol_release,"host_config":path("host-config.json"),"lot":{"lot_id":lot_id.to_string(),"lot_location":location,"name":s.lot_name},"catalog":{"release":s.binding_sha256,"entries":null},"avatar_specs":null});
    let result = json!({"schema":"wonderland.authored-lot-genesis-result.v1","genesis_input_sha256":hex(&receipt_hash),"checkpoint_sha256":Digest::of(&checkpoint),"cursor":cursor,"entities":0,"owned_actors":0,"admission_initialized":true,"source_default_unowned_permissions":true,"account_authentication_or_custody_granted":false,"catalog_and_avatar_bank_required":true});
    fs::create_dir(output)?;
    for (name, bytes) in [
        ("operator-settings.json", settings_bytes),
        ("authored-empty-lot.xml", xml.into_bytes()),
        ("lot.bin", lot_bytes),
        ("world.json", appearance_bytes),
        ("genesis-input.json", receipt_bytes),
        ("checkpoint.bin", checkpoint),
        ("host-config.json", serde_json::to_vec_pretty(&host)?),
        (
            "manifest.pending.json",
            serde_json::to_vec_pretty(&pending)?,
        ),
        ("genesis-result.json", serde_json::to_vec_pretty(&result)?),
    ] {
        fs::write(output.join(name), bytes)?;
    }
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("authored lot genesis refused: {error}");
        std::process::exit(1)
    }
}
