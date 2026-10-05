use wonderland_client_app::authoring::*;
use wonderland_contracts::Availability;
use wonderland_contracts::authoring::*;

fn wardrobe_projection() -> AuthoringProjection {
    let mut projection = preview_authoring_projection();
    let wardrobe = &mut projection.profiles[0].wardrobe;
    wardrobe.categories = vec![WardrobeCategory {
        id: "day".into(),
        label: "Day".into(),
        slot: AppearanceSlot::Body,
    }];
    wardrobe.outfits = ["first", "second", "third"]
        .into_iter()
        .map(|id| OwnedOutfit {
            id: id.into(),
            content_key: format!("test:{id}").into(),
            category_id: "day".into(),
            label: id.into(),
            thumbnail: None,
            is_default: id == "first",
            actions: vec![WardrobeActionOffer {
                action: WardrobeAction::Delete,
                availability: Availability::Available,
            }],
        })
        .collect();
    projection
}

fn delete_default(state: &mut AuthoringState) -> AuthoringRequest {
    state
        .dispatch(AuthoringIntent::SelectProfile("maya".into()))
        .unwrap();
    state.dispatch(AuthoringIntent::OpenOutfit).unwrap();
    state
        .dispatch(AuthoringIntent::SelectOwnedOutfit("first".into()))
        .unwrap();
    state
        .dispatch(AuthoringIntent::SelectWardrobeAction(
            WardrobeAction::Delete,
        ))
        .unwrap();
    state
        .dispatch(AuthoringIntent::SaveOutfit)
        .unwrap()
        .remove(0)
}

#[test]
fn deleting_a_default_selects_a_surviving_outfit_in_the_same_category() {
    let projection = wardrobe_projection();
    let mut provider = PreviewAuthoringProvider::new(projection.clone()).unwrap();
    let mut state = AuthoringState::new(projection.clone());
    let request = delete_default(&mut state);
    let event = provider.handle(&request);
    let value = serde_json::to_value(&event).unwrap();
    assert_eq!(value["outcome"]["replacement_default"], "second");
    state.receive(event).unwrap();
    let wardrobe = &state.projection().profiles[0].wardrobe;
    assert!(wardrobe.outfit(&"first".into()).is_none());
    assert!(wardrobe.outfit(&"second".into()).unwrap().is_default);
    assert!(!wardrobe.outfit(&"third".into()).unwrap().is_default);
    assert_eq!(&state.projection().profiles[1..], &projection.profiles[1..]);
    assert_eq!(
        state.projection().profiles[0].appearance,
        projection.profiles[0].appearance
    );
}

#[test]
fn a_source_may_choose_another_same_category_default_with_an_exact_receipt() {
    let projection = wardrobe_projection();
    let mut provider = PreviewAuthoringProvider::new(projection.clone()).unwrap();
    let mut state = AuthoringState::new(projection);
    let request = delete_default(&mut state);
    let mut value = serde_json::to_value(provider.handle(&request)).unwrap();
    value["outcome"]["replacement_default"] = "third".into();
    let outfits = value["projection"]["profiles"][0]["wardrobe"]["outfits"]
        .as_array_mut()
        .unwrap();
    for outfit in outfits {
        outfit["is_default"] = (outfit["id"] == "third").into();
    }
    let event = serde_json::from_value(value).unwrap();
    state.receive(event).unwrap();
    assert!(
        state.projection().profiles[0]
            .wardrobe
            .outfit(&"third".into())
            .unwrap()
            .is_default
    );
}

#[test]
fn deleting_the_only_default_is_rejected_without_losing_the_draft() {
    let mut projection = wardrobe_projection();
    projection.profiles[0].wardrobe.outfits.truncate(1);
    let mut state = AuthoringState::new(projection.clone());
    state
        .dispatch(AuthoringIntent::SelectProfile("maya".into()))
        .unwrap();
    state.dispatch(AuthoringIntent::OpenOutfit).unwrap();
    state
        .dispatch(AuthoringIntent::SelectWardrobeAction(
            WardrobeAction::Delete,
        ))
        .unwrap();
    assert!(matches!(
        state.dispatch(AuthoringIntent::SaveOutfit),
        Err(AuthoringError::Unavailable(_))
    ));
    assert!(state.draft().is_some());
    assert_eq!(state.projection(), &projection);
}

fn grant(actor: &str) -> serde_json::Value {
    serde_json::json!({
        "actor_id": actor,
        "purchase": {
            "availability": { "status": "available" },
            "payer_id": actor,
            "object_owner_id": actor,
            "categories": null
        },
        "arrange": { "status": "available" },
        "build": { "status": "available" },
        "inventory_owners": [actor]
    })
}

fn shared_value() -> serde_json::Value {
    let mut value = serde_json::to_value(preview_authoring_projection()).unwrap();
    let profiles = value["profiles"].as_array_mut().unwrap();
    for profile in profiles.iter_mut() {
        profile["home"]["grants"] =
            serde_json::json!([grant(profile["character"]["id"].as_str().unwrap())]);
    }
    profiles[0]["home"]["grants"]
        .as_array_mut()
        .unwrap()
        .push(grant("jules"));
    value
}

fn shared_projection() -> AuthoringProjection {
    serde_json::from_value(shared_value()).unwrap()
}

fn purchase(
    state: &mut AuthoringState,
    actor: &str,
    home: &str,
    item: &str,
) -> Result<AuthoringRequest, AuthoringError> {
    state.dispatch(AuthoringIntent::SelectProfile(actor.into()))?;
    state.dispatch(AuthoringIntent::OpenHome(home.into()))?;
    state.dispatch(AuthoringIntent::SelectCatalog(item.into()))?;
    state.dispatch(AuthoringIntent::SetCell(GridCell { x: 2, y: 2 }))?;
    Ok(state.dispatch(AuthoringIntent::ConfirmPlacement)?.remove(0))
}

#[test]
fn roommate_purchase_sendback_and_replacement_keep_the_payer_and_object_owner() {
    let projection = shared_projection();
    let maya_money = projection.profile(&"maya".into()).unwrap().character.money;
    let jules_money = projection.profile(&"jules".into()).unwrap().character.money;
    let mut provider = PreviewAuthoringProvider::new(projection.clone()).unwrap();
    let mut state = AuthoringState::new(projection);
    let request = purchase(&mut state, "jules", "maya", "armchair").unwrap();
    state.receive(provider.handle(&request)).unwrap();
    let bought = state.projection().home(&"maya".into()).unwrap().instances[0].clone();
    assert_eq!(serde_json::to_value(&bought).unwrap()["owner_id"], "jules");
    assert_eq!(
        state
            .projection()
            .profile(&"maya".into())
            .unwrap()
            .character
            .money,
        maya_money
    );
    assert_eq!(
        state
            .projection()
            .profile(&"jules".into())
            .unwrap()
            .character
            .money,
        jules_money - 180
    );

    state
        .dispatch(AuthoringIntent::SelectProfile("maya".into()))
        .unwrap();
    state
        .dispatch(AuthoringIntent::OpenHome("maya".into()))
        .unwrap();
    state
        .dispatch(AuthoringIntent::SelectOwned(bought.id.clone()))
        .unwrap();
    let request = state
        .dispatch(AuthoringIntent::StoreSelected)
        .unwrap()
        .remove(0);
    state.receive(provider.handle(&request)).unwrap();
    let stored = state
        .projection()
        .home(&"maya".into())
        .unwrap()
        .instance(&bought.id)
        .unwrap();
    assert!(stored.placement.is_none());
    assert_eq!(serde_json::to_value(stored).unwrap()["owner_id"], "jules");
    assert!(matches!(
        state
            .dispatch(AuthoringIntent::BeginPlace(bought.id.clone()))
            .and_then(|_| state.dispatch(AuthoringIntent::ConfirmPlacement)),
        Err(AuthoringError::PermissionDenied)
    ));
    state.dispatch(AuthoringIntent::Cancel).unwrap();

    state
        .dispatch(AuthoringIntent::SelectProfile("jules".into()))
        .unwrap();
    state
        .dispatch(AuthoringIntent::OpenHome("jules".into()))
        .unwrap();
    state
        .dispatch(AuthoringIntent::BeginPlace(bought.id.clone()))
        .unwrap();
    let request = state
        .dispatch(AuthoringIntent::ConfirmPlacement)
        .unwrap()
        .remove(0);
    state.receive(provider.handle(&request)).unwrap();
    assert!(
        state
            .projection()
            .home(&"maya".into())
            .unwrap()
            .instance(&bought.id)
            .is_none()
    );
    let placed = state
        .projection()
        .home(&"jules".into())
        .unwrap()
        .instance(&bought.id)
        .unwrap();
    assert!(placed.placement.is_some());
    assert_eq!(serde_json::to_value(placed).unwrap()["owner_id"], "jules");
    assert_eq!(
        state
            .projection()
            .profile(&"jules".into())
            .unwrap()
            .character
            .money,
        jules_money - 180
    );
}

#[test]
fn unscoped_lot_availability_does_not_authorize_even_the_owner() {
    let mut value = serde_json::to_value(preview_authoring_projection()).unwrap();
    value["profiles"][0]["home"]["grants"] = serde_json::json!([]);
    let projection = serde_json::from_value(value).unwrap();
    let mut state = AuthoringState::new(projection);
    assert!(matches!(
        purchase(&mut state, "maya", "maya", "armchair"),
        Err(AuthoringError::PermissionDenied)
    ));
}

#[test]
fn a_granted_external_lot_does_not_require_its_owner_in_the_playable_roster() {
    let mut value = shared_value();
    let mut external = value["profiles"][0]["home"].clone();
    external["owner_id"] = "neighbor".into();
    external["grants"] = serde_json::json!([grant("jules")]);
    external["build"]["preview"] = serde_json::json!({"status": "available"});
    external["build"]["tools"][3]["availability"] = serde_json::json!({"status": "available"});
    value["shared_homes"] = serde_json::json!([external]);
    let projection: AuthoringProjection = serde_json::from_value(value).unwrap();
    assert!(projection.profile(&"neighbor".into()).is_none());
    let home = projection
        .home(&"neighbor".into())
        .expect("source supplied external target lot");
    let request = ArchitectureRequest {
        operation_id: "roommate-walls-preview".into(),
        base_revision: projection.revision,
        lot_revision: home.lot.revision,
        build_revision: home.build.revision,
        actor_id: "jules".into(),
        home_owner_id: "neighbor".into(),
        phase: ArchitecturePhase::Preview,
        edit: ArchitectureEdit {
            tool_id: "walls".into(),
            content_key: None,
            from: LotCell {
                cell: GridCell { x: 1, y: 1 },
                level: 0,
            },
            to: LotCell {
                cell: GridCell { x: 2, y: 1 },
                level: 0,
            },
            direction: Direction::North,
            value: None,
        },
    };
    request.validate(&projection).unwrap();
    let mut provider = PreviewAuthoringProvider::new(projection.clone()).unwrap();
    let mut state = AuthoringState::new(projection);
    let request = purchase(&mut state, "jules", "neighbor", "floor-lamp").unwrap();
    state.receive(provider.handle(&request)).unwrap();
    assert_eq!(state.projection().profiles.len(), 5);
    assert_eq!(
        state
            .projection()
            .home(&"neighbor".into())
            .unwrap()
            .instances
            .len(),
        1
    );
}

#[test]
fn a_source_purchase_category_scope_is_enforced_on_the_target_lot() {
    let mut value = shared_value();
    value["profiles"][0]["home"]["grants"][1]["purchase"]["categories"] =
        serde_json::json!(["lighting"]);
    let projection: AuthoringProjection = serde_json::from_value(value).unwrap();
    let mut state = AuthoringState::new(projection.clone());
    assert!(matches!(
        purchase(&mut state, "jules", "maya", "armchair"),
        Err(AuthoringError::PermissionDenied)
    ));
    state.dispatch(AuthoringIntent::Cancel).unwrap();
    let request = purchase(&mut state, "jules", "maya", "floor-lamp").unwrap();
    let mut provider = PreviewAuthoringProvider::new(projection).unwrap();
    state.receive(provider.handle(&request)).unwrap();
    assert_eq!(
        state
            .projection()
            .home(&"maya".into())
            .unwrap()
            .instances
            .len(),
        1
    );
}

#[test]
fn an_old_v2_preview_resumes_known_owner_inventory_only_in_the_preview_adapter() {
    let mut value = serde_json::to_value(preview_authoring_projection()).unwrap();
    for profile in value["profiles"].as_array_mut().unwrap() {
        profile["home"].as_object_mut().unwrap().remove("grants");
    }
    value["profiles"][0]["home"]["instances"] = serde_json::json!([{
        "id": "old-v2-chair", "catalog_id": "armchair", "placement": null
    }]);
    let decoded: AuthoringProjection = serde_json::from_value(value).unwrap();
    decoded.validate().unwrap();
    assert!(decoded.profiles[0].home.grants.is_empty());
    assert!(decoded.profiles[0].home.instances[0].owner_id.is_none());
    let resumed = resume_preview_projection(decoded.clone());
    assert_eq!(
        resumed.profiles[0].home.instances[0]
            .owner_id
            .as_ref()
            .unwrap()
            .as_ref(),
        "maya"
    );
    assert_eq!(resumed.profiles[0].portrait, decoded.profiles[0].portrait);
    assert_eq!(resumed.profiles[0].character, decoded.profiles[0].character);
    let mut provider = PreviewAuthoringProvider::new(resumed.clone()).unwrap();
    let mut state = AuthoringState::new(resumed);
    state
        .dispatch(AuthoringIntent::SelectProfile("maya".into()))
        .unwrap();
    state
        .dispatch(AuthoringIntent::OpenHome("maya".into()))
        .unwrap();
    state
        .dispatch(AuthoringIntent::BeginPlace("old-v2-chair".into()))
        .unwrap();
    let request = state
        .dispatch(AuthoringIntent::ConfirmPlacement)
        .unwrap()
        .remove(0);
    state.receive(provider.handle(&request)).unwrap();
    assert_eq!(
        state.projection().profiles[0].home.instances[0].id.as_ref(),
        "old-v2-chair"
    );

    let mut live = decoded;
    live.account.source = "source-account-service".into();
    assert_eq!(resume_preview_projection(live.clone()), live);
}
