//! The cooked bytes are the only content inputs after the source directory is removed.
use wonderland_content_ir::manifest::{AssetManifest, Digest};
use wonderland_content_runtime_bridge::{
    cooked::{CookedLoadLimits, PackBytes, PreparedDraft, PreparedRelease},
    isolated::{IsolatedRuntime, RoutineQuery},
    sim_core::{
        ids::{ObjectId, PersistentId},
        runtime::{AcceptedCommand, RuntimeConfig, RuntimeRole, SimRuntime, SpawnSpec},
        snapshot::SnapshotExpectation,
        vm::{FrameContext, PrimitiveExit, RoutineKey, RoutineScope, VmMode, VmStop},
        world::{Facing, LotModel, TilePos},
    },
};

#[path = "support/cooked_fixture.rs"]
mod fixture;

fn pack_refs(fixture: &fixture::Fixture) -> Vec<PackBytes<'_>> {
    fixture
        .packs
        .iter()
        .map(|(digest, bytes)| PackBytes { digest, bytes })
        .collect()
}

#[test]
fn patched_tuned_object_runs_from_selected_packs_after_sources_are_removed() {
    let fixture = fixture::cook();
    assert!(!fixture.removed_source.exists());
    let limits = CookedLoadLimits::default();
    assert!(
        AssetManifest::from_json(&fixture.manifest, &limits.manifest)
            .unwrap()
            .packs
            .len()
            > 1
    );
    let packs = pack_refs(&fixture);
    let draft = PreparedDraft::from_json(&fixture.draft, &fixture.manifest, &limits).unwrap();
    let sealed = draft.seal(&packs, &limits).unwrap();
    let binding = sealed.canonical_bytes(&limits).unwrap();
    let prepared =
        PreparedRelease::from_binding(&binding, &Digest::of(&binding), &fixture.manifest, &limits)
            .unwrap();
    assert_eq!(prepared.required_packs().len(), 1);
    let loaded = prepared.load(&packs, &limits).unwrap();
    assert_eq!(loaded.content.tuning().values[&(123, 4096, 0)], 37);
    assert_eq!(
        loaded.content.named_tree(123, "Named behavior").unwrap().id,
        4097
    );
    assert_eq!(loaded.report.manifest_sha256, Digest::of(&fixture.manifest));
    assert_eq!(loaded.report.binding_sha256, Digest::of(&binding));
    let mut runtime = SimRuntime::new(
        loaded.content,
        LotModel::new(8, 8, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 11, 7, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let tick = runtime
        .next_tick(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: 123,
            position: TilePos::new(3, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar: false,
        })])
        .unwrap();
    runtime.step(&tick).unwrap();
    let actor = runtime.state().entities[&ObjectId(1)].info.reference;
    let snapshot = runtime.snapshot().unwrap();
    let mut replica = IsolatedRuntime::from_snapshot(
        &snapshot,
        runtime.content().clone(),
        SnapshotExpectation::new(11, 7),
    )
    .unwrap();
    let query = replica
        .query(&RoutineQuery {
            actor,
            target: actor,
            code_owner: 123,
            routine_id: 4096,
            args: vec![],
            instruction_budget: 10,
        })
        .unwrap();
    assert_eq!(query.stop, VmStop::Completed(PrimitiveExit::ReturnTrue));
    assert_eq!(query.instructions, 3);
    assert_eq!(&query.temps[..2], &[37, 41]);
    assert_eq!(replica.runtime().snapshot().unwrap(), snapshot);
    let tick = runtime
        .next_tick(vec![AcceptedCommand::StartBehavior {
            entity: actor,
            routine: RoutineKey {
                scope: RoutineScope::Private(123),
                id: 4096,
            },
            context: FrameContext::for_entity(actor, 123),
            args: vec![],
            replace: true,
        }])
        .unwrap();
    let authority = runtime.step(&tick).unwrap();
    let replayed = replica.advance(&tick).unwrap();
    assert_eq!(authority.state_hash, replayed.state_hash);
    assert!(replayed.effects.is_empty());
    assert_eq!(
        replica.runtime().state().entities[&ObjectId(1)].attributes,
        [41]
    );
    assert_eq!(
        runtime.snapshot().unwrap(),
        replica.runtime().snapshot().unwrap()
    );
}

fn binding(
    fixture: &fixture::Fixture,
) -> wonderland_content_runtime_bridge::cooked::CookedRuntimeBindingV1 {
    let limits = CookedLoadLimits::default();
    PreparedDraft::from_json(&fixture.draft, &fixture.manifest, &limits)
        .unwrap()
        .seal(&pack_refs(fixture), &limits)
        .unwrap()
}

#[test]
fn hashes_pack_members_and_sealed_descriptor_are_required_before_content_is_exposed() {
    let fixture = fixture::cook();
    let limits = CookedLoadLimits::default();
    let mut sealed = binding(&fixture);
    let bytes = sealed.canonical_bytes(&limits).unwrap();
    assert!(PreparedRelease::from_binding(
        &bytes,
        &Digest::of(b"untrusted"),
        &fixture.manifest,
        &limits
    )
    .is_err());
    let mut different_manifest = fixture.manifest.clone();
    different_manifest.push(b' ');
    assert!(PreparedRelease::from_binding(
        &bytes,
        &Digest::of(&bytes),
        &different_manifest,
        &limits
    )
    .is_err());
    let prepared =
        PreparedRelease::from_binding(&bytes, &Digest::of(&bytes), &fixture.manifest, &limits)
            .unwrap();
    assert!(prepared
        .load(&[], &limits)
        .unwrap_err()
        .contains("exactly cover"));
    let (digest, pack) = fixture.packs.iter().next().unwrap();
    let mut corrupted = pack.clone();
    *corrupted.last_mut().unwrap() ^= 1;
    assert!(prepared
        .load(
            &[PackBytes {
                digest,
                bytes: &corrupted
            }],
            &limits
        )
        .unwrap_err()
        .contains("digest"));
    let unrelated = Digest::of(b"not the selected pack");
    assert!(prepared
        .load(
            &[PackBytes {
                digest: &unrelated,
                bytes: pack
            }],
            &limits
        )
        .unwrap_err()
        .contains("identity"));
    assert!(PreparedRelease::from_binding(
        &fixture.draft,
        &Digest::of(&fixture.draft),
        &fixture.manifest,
        &limits
    )
    .is_err());
    // Both routines have the same name. An explicitly rehashed but reordered
    // binding changes first-name resolution and must fail its sealed descriptor.
    sealed.recipe.scopes[0].resources.swap(0, 2);
    let changed = sealed.canonical_bytes(&limits).unwrap();
    let changed =
        PreparedRelease::from_binding(&changed, &Digest::of(&changed), &fixture.manifest, &limits)
            .unwrap();
    assert!(changed
        .load(&pack_refs(&fixture), &limits)
        .unwrap_err()
        .contains("descriptor"));
    prepared.load(&pack_refs(&fixture), &limits).unwrap();
}

#[test]
fn first_glob_in_source_order_controls_presence_even_when_its_name_is_empty() {
    use wonderland_legacy_formats::{
        iff::{ChunkKey, IffChunk},
        semantic::{encode_glob, Glob, GlobEncoding, LegacyString, TextEncoding},
        Limits,
    };
    let fixture = fixture::cook_custom(|file| {
        for (id, name) in [(0, ""), (1, "MustNotBeSelected")] {
            file.chunks.push(IffChunk {
                key: ChunkKey { kind: *b"GLOB", id },
                flags: 0,
                label: [0; 64],
                data: encode_glob(
                    &Glob {
                        name: LegacyString::from_text(name, TextEncoding::Ascii).unwrap(),
                        storage: GlobEncoding::Pascal,
                        terminated: false,
                        trailing: vec![],
                    },
                    &Limits::default(),
                )
                .unwrap(),
            });
        }
    });
    let sealed = binding(&fixture);
    let limits = CookedLoadLimits::default();
    let bytes = sealed.canonical_bytes(&limits).unwrap();
    let prepared =
        PreparedRelease::from_binding(&bytes, &Digest::of(&bytes), &fixture.manifest, &limits)
            .unwrap();
    assert!(prepared
        .load(&pack_refs(&fixture), &limits)
        .unwrap()
        .content
        .object(123)
        .is_some());
}

#[test]
fn objf_remains_a_verified_semantic_member_and_reaches_runtime_entrypoints() {
    use wonderland_legacy_formats::{
        iff::{ChunkKey, IffChunk},
        semantic::{decode_objd, encode_objd, encode_objf, Objf, ObjfFunction},
        Limits,
    };
    let fixture = fixture::cook_custom(|file| {
        let chunk = file
            .chunks
            .iter_mut()
            .find(|c| c.key.kind == *b"OBJD")
            .unwrap();
        let mut object = decode_objd(&chunk.data, &Limits::default()).unwrap();
        object.set_field("UsesFnTable", 1).unwrap();
        chunk.data = encode_objd(&object, &Limits::default()).unwrap();
        let mut functions = vec![
            ObjfFunction {
                condition: 0,
                action: 0
            };
            10
        ];
        functions[9] = ObjfFunction {
            condition: 4097,
            action: 4096,
        };
        file.chunks.push(IffChunk {
            key: ChunkKey {
                kind: *b"OBJf",
                id: 128,
            },
            flags: 0,
            label: [0; 64],
            data: encode_objf(
                &Objf {
                    padding: 42,
                    version: 7,
                    functions,
                    trailing: vec![],
                },
                &Limits::default(),
            )
            .unwrap(),
        });
    });
    let sealed = binding(&fixture);
    let limits = CookedLoadLimits::default();
    let bytes = sealed.canonical_bytes(&limits).unwrap();
    let loaded =
        PreparedRelease::from_binding(&bytes, &Digest::of(&bytes), &fixture.manifest, &limits)
            .unwrap()
            .load(&pack_refs(&fixture), &limits)
            .unwrap();
    let object = loaded.content.object(123).unwrap();
    assert_eq!(object.entry_point_count, 10);
    assert_eq!(object.entry_points[&9].id, 4096);
    assert_eq!(object.entry_conditions[&9].id, 4097);
}
