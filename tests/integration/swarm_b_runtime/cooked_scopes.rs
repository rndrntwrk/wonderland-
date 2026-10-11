//! Execute all three source namespaces using only verified production-cooker outputs.
use wonderland_content_ir::manifest::Digest;
use wonderland_content_runtime_bridge::{
    cooked::{
        CookedLoadLimits, CookedRuntimeDraftV1, PackBytes, PreparedDraft, PreparedRelease,
        ScopeNamespaceV1,
    },
    isolated::{IsolatedRuntime, RoutineQuery},
    sim_core::{
        ids::{ObjectId, PersistentId},
        runtime::{AcceptedCommand, RuntimeConfig, RuntimeRole, SimRuntime, SpawnSpec},
        vm::{PrimitiveExit, RoutineScope, VmMode, VmStop},
        world::{Facing, LotModel, TilePos},
    },
};

#[path = "support/cooked_scopes_fixture.rs"]
mod fixture;
use fixture::{OBJECT_GUID, SHARED_OWNER};

fn packs(fixture: &fixture::Fixture) -> Vec<PackBytes<'_>> {
    fixture
        .packs
        .iter()
        .map(|(digest, bytes)| PackBytes { digest, bytes })
        .collect()
}

#[test]
fn private_calls_semiglobal_then_global_from_cooked_members_and_tuning() {
    let fixture = fixture::cook();
    assert!(!fixture.removed_source.exists());
    let limits = CookedLoadLimits::default();
    let draft = serde_json::to_vec(&fixture.draft).unwrap();
    let sealed = PreparedDraft::from_json(&draft, &fixture.manifest, &limits)
        .unwrap()
        .seal(&packs(&fixture), &limits)
        .unwrap();
    let binding = sealed.canonical_bytes(&limits).unwrap();
    let loaded =
        PreparedRelease::from_binding(&binding, &Digest::of(&binding), &fixture.manifest, &limits)
            .unwrap()
            .load(&packs(&fixture), &limits)
            .unwrap();
    assert_eq!(loaded.content.tuning().values[&(OBJECT_GUID, 4096, 0)], 17);
    // The shared source's BCON is 31; the production resolver applies the
    // authored dynamic 37.9 override, and the packed tuning retains 37.
    assert_eq!(loaded.content.tuning().values[&(SHARED_OWNER, 8192, 0)], 37);
    assert_eq!(loaded.content.tuning().values[&(0, 256, 0)], 23);
    assert_eq!(
        loaded
            .content
            .named_tree(OBJECT_GUID, "Private behavior")
            .unwrap()
            .scope,
        RoutineScope::Private(OBJECT_GUID)
    );
    assert_eq!(
        loaded
            .content
            .named_tree(OBJECT_GUID, "Shared behavior")
            .unwrap()
            .scope,
        RoutineScope::SemiGlobal(SHARED_OWNER)
    );
    assert_eq!(
        loaded
            .content
            .named_tree(0, "Global behavior")
            .unwrap()
            .scope,
        RoutineScope::Global
    );
    assert_eq!(
        loaded.report.objects[0].semiglobal_owner,
        Some(SHARED_OWNER)
    );

    let mut runtime = SimRuntime::new(
        loaded.content,
        LotModel::new(8, 8, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 11, 7, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let tick = runtime
        .next_tick(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: OBJECT_GUID,
            position: TilePos::new(3, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar: false,
        })])
        .unwrap();
    runtime.step(&tick).unwrap();
    let actor = runtime.state().entities[&ObjectId(1)].info.reference;
    let original = runtime.snapshot().unwrap();
    let isolated = IsolatedRuntime::capture(&runtime).unwrap();
    let outcome = isolated
        .query(&RoutineQuery {
            actor,
            target: actor,
            code_owner: OBJECT_GUID,
            routine_id: 4096,
            args: vec![],
            instruction_budget: 8,
        })
        .unwrap();
    assert_eq!(outcome.stop, VmStop::Completed(PrimitiveExit::ReturnTrue));
    assert_eq!(outcome.instructions, 3);
    assert_eq!(&outcome.temps[..3], &[0, 0, 61]);
    assert_eq!(isolated.runtime().snapshot().unwrap(), original);
    assert_eq!(runtime.snapshot().unwrap(), original);
}

fn rejected(fixture: &fixture::Fixture, draft: &CookedRuntimeDraftV1) -> String {
    let limits = CookedLoadLimits::default();
    let bytes = serde_json::to_vec(draft).unwrap();
    match PreparedDraft::from_json(&bytes, &fixture.manifest, &limits) {
        Err(error) => error,
        Ok(prepared) => prepared.seal(&packs(fixture), &limits).unwrap_err(),
    }
}

#[test]
fn verified_members_cannot_supply_a_wrong_or_missing_bound_shared_scope() {
    let fixture = fixture::cook();
    let mut wrong_name = fixture.draft.clone();
    wrong_name.recipe.scopes[1].source_name = "Different.iff".into();
    assert!(rejected(&fixture, &wrong_name).contains("does not match GLOB"));

    for namespace in [ScopeNamespaceV1::Semiglobal, ScopeNamespaceV1::Global] {
        let mut missing = fixture.draft.clone();
        missing
            .recipe
            .scopes
            .retain(|scope| scope.namespace != namespace);
        match namespace {
            ScopeNamespaceV1::Semiglobal => missing.recipe.objects[0].semiglobal = None,
            ScopeNamespaceV1::Global => missing.recipe.objects[0].global_scope = None,
            ScopeNamespaceV1::Private => unreachable!(),
        }
        // These omitted members remain in the verified pack. Runtime scope
        // presence must come from the binding and packed tuning, not pack search.
        assert!(rejected(&fixture, &missing).contains("scope presence"));
    }
}
