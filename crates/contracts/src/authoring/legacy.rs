//! Explicit migration of the local v1 illustration preview, never a live account
//! adapter. Historical fields are preserved without fabricating game outfit IDs.
use super::*;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct LegacyProjection {
    version: u32,
    revision: u64,
    profiles: Vec<LegacyProfile>,
    catalog: Vec<LegacyCatalogItem>,
}

#[derive(Deserialize)]
struct LegacyProfile {
    character: Character,
    identity: String,
    look_id: String,
    home: LegacyHome,
}

#[derive(Deserialize)]
struct LegacyHome {
    owner_id: CharacterId,
    permissions: HomePermissions,
    instances: Vec<OwnedInstance>,
}

#[derive(Deserialize)]
struct LegacyCatalogItem {
    id: CatalogId,
    name: String,
    category: CatalogCategoryId,
    price: i64,
    footprint: Footprint,
    availability: Availability,
}

pub fn migrate_v1_projection(json: &str) -> Result<AuthoringProjection, AuthoringError> {
    if json.len() > MAX_AUTHORING_JSON_BYTES {
        return Err(AuthoringError::SafetyLimit);
    }
    let old: LegacyProjection = serde_json::from_str(json)
        .map_err(|error| AuthoringError::InvalidProjection(format!("Saved v1 data could not be read: {error}")))?;
    if old.version != 1 {
        return Err(AuthoringError::InvalidProjection("Expected a v1 saved projection".into()));
    }
    let unavailable = Availability::Unavailable {
        reason: "The original game content and service have not been connected".into(),
    };
    let account = AccountCapabilities {
        source: "legacy-preview-migration".into(),
        revision: 1,
        account_id: None,
        shards: vec![],
        default_shard: None,
        creation: unavailable.clone(),
        profile_capacity: CapacityPolicy::Unknown,
        fields: ProfileFieldPolicy {
            minimum_name_characters: 1,
            maximum_name_characters: 32,
            name_alphabet: NameAlphabet::Unicode,
            maximum_description_characters: 499,
        },
    };
    let appearance_content = AppearanceContent {
        source: "legacy-preview-migration".into(),
        revision: 1,
        heads: vec![],
        bodies: vec![],
        skin_tones: vec![],
        genders: vec![],
        requirements: AppearanceRequirements::default(),
        rendering: unavailable.clone(),
    };
    let mut categories = BTreeMap::new();
    let catalog = old.catalog.into_iter().map(|item| {
        categories.entry(item.category.clone()).or_insert_with(|| item.category.to_string());
        // The old preview's artwork rules are converted once into metadata.
        let rotations = if matches!(item.id.as_ref(), "armchair" | "coffee-table" | "bookcase") {
            vec![Direction::North, Direction::East, Direction::South, Direction::West]
        } else {
            vec![Direction::North]
        };
        CatalogItem {
            source_key: item.id.to_string().into(),
            id: item.id,
            name: item.name,
            category: item.category,
            price: item.price,
            footprint: item.footprint,
            rotations,
            thumbnail: None,
            availability: item.availability,
        }
    }).collect();
    let profiles = old.profiles.into_iter().map(|profile| {
        let asset_path = if profile.look_id == format!("{}-everyday", profile.identity) {
            format!("/assets/art/{}.png", profile.identity)
        } else {
            format!("/assets/authoring/{}.png", profile.look_id)
        };
        AuthoringProfile {
            character: profile.character,
            description: String::new(),
            shard_id: None,
            appearance: AppearanceSelection::default(),
            portrait: Some(PortraitReference {
                source: "legacy-preview-artwork".into(),
                identity: profile.identity,
                look_id: profile.look_id,
                asset_path,
            }),
            wardrobe: Wardrobe { revision: 1, categories: vec![], outfits: vec![] },
            home: Home {
                owner_id: profile.home.owner_id,
                permissions: profile.home.permissions,
                instance_capacity: CapacityPolicy::Unknown,
                lot: LotGeometry {
                    source: "legacy-preview-grid".into(),
                    revision: old.revision,
                    bounds: LotBounds { origin: GridCell { x: 0, y: 0 }, width: 8, depth: 6 },
                    levels: vec![0],
                    reserved: vec![LotCell { cell: GridCell { x: 0, y: 0 }, level: 0 }],
                },
                build: BuildCapabilities {
                    source: "legacy-preview-migration".into(),
                    revision: 1,
                    tools: vec![],
                    view_modes: vec![],
                    preview: unavailable.clone(),
                    commit: unavailable.clone(),
                },
                instances: profile.home.instances,
            },
        }
    }).collect();
    let projection = AuthoringProjection {
        version: AUTHORING_VERSION,
        revision: old.revision,
        account,
        appearance_content,
        catalog_source: "legacy-preview-catalog".into(),
        catalog_revision: 1,
        catalog_categories: categories.into_iter().map(|(id, label)| CatalogCategory { id, label }).collect(),
        profiles,
        catalog,
    };
    projection.validate()?;
    Ok(projection)
}
