//! Read-only catalogue and disposable map choices for native construction.
use crate::world_draft::WorldDraftOutline;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use wonderland_game_runtime::live_wire::{
    construction::{ConstructionRequest, Selection},
    player::PlayerBinding,
};
use wonderland_game_runtime::{Facing, TilePos};
use wonderland_render_core::Vec3;
use wonderland_world_view::{
    WorldDocument, WorldPick, WorldPickTarget, WorldRevision, WorldSurface,
};
pub const MAX_CATALOGUE_BYTES: usize = 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ItemKind {
    Object { catalog_id: u32 },
    Floor { pattern: u16 },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub kind: ItemKind,
    pub label: String,
    pub category: String,
    pub price: i64,
    pub thumbnail: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalogue {
    pub version: u8,
    pub binding: PlayerBinding,
    pub lot_id: u64,
    pub authority_epoch: u64,
    pub content_hash: [u8; 32],
    pub permissions_revision: u64,
    pub account_revision: u64,
    pub catalog_revision: u64,
    pub balance: i64,
    pub can_build: bool,
    pub next_request_id: u64,
    pub csrf_token: String,
    pub items: Vec<Item>,
}
impl std::fmt::Debug for Catalogue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Catalogue")
            .field("binding", &self.binding)
            .field("item_count", &self.items.len())
            .field("csrf_token", &"[redacted]")
            .finish_non_exhaustive()
    }
}
impl Item {
    pub fn key(&self) -> String {
        match self.kind {
            ItemKind::Object { catalog_id } => format!("object-{catalog_id}"),
            ItemKind::Floor { pattern } => format!("floor-{pattern}"),
        }
    }
}
impl Catalogue {
    pub fn decode(
        bytes: &[u8],
        binding: PlayerBinding,
        world: &WorldDocument,
    ) -> Result<Self, &'static str> {
        if bytes.len() > MAX_CATALOGUE_BYTES {
            return Err("Catalogue is too large");
        }
        let c: Self = serde_json::from_slice(bytes).map_err(|_| "Invalid catalogue")?;
        c.validate(binding, world)?;
        Ok(c)
    }
    pub fn validate(
        &self,
        binding: PlayerBinding,
        world: &WorldDocument,
    ) -> Result<(), &'static str> {
        if self.version != 1
            || self.binding != binding
            || binding.avatar_id == 0
            || binding.lot_location == 0
            || binding.source_epoch == 0
            || binding.lot_incarnation == 0
            || self.lot_id == 0
            || self.authority_epoch == 0
            || world.revision.lot_id != Some(self.lot_id)
            || world.revision.epoch != self.authority_epoch
            || world.revision.content.0 != self.content_hash
        {
            return Err("The catalogue belongs to a different property session");
        }
        if self.balance < 0
            || self.next_request_id == 0
            || self.items.len() > 8192
            || self.csrf_token.is_empty()
            || self.csrf_token.len() > 4096
            || !self.csrf_token.bytes().all(|b| (0x21..=0x7e).contains(&b))
        {
            return Err("Invalid catalogue metadata");
        }
        let mut keys = BTreeSet::new();
        for item in &self.items {
            let valid_id = match item.kind {
                ItemKind::Object { catalog_id } => catalog_id != 0,
                ItemKind::Floor { pattern } => pattern != 0,
            };
            if !valid_id
                || !keys.insert(item.key())
                || item.price < 0
                || !valid_text(&item.label, 160)
                || !valid_text(&item.category, 80)
                || item.thumbnail.as_ref().is_some_and(|p| !valid_thumbnail(p))
            {
                return Err("Invalid catalogue item");
            }
        }
        Ok(())
    }
    pub fn page(&self, query: &str, category: &str, page: usize, size: usize) -> Vec<Item> {
        let size = size.min(48);
        let Some(start) = page.checked_mul(size) else {
            return Vec::new();
        };
        let query = query.trim().to_lowercase();
        self.items
            .iter()
            .filter(|i| {
                (category.is_empty() || i.category == category)
                    && (query.is_empty() || i.label.to_lowercase().contains(&query))
            })
            .skip(start)
            .take(size)
            .cloned()
            .collect()
    }
}
fn valid_text(text: &str, max: usize) -> bool {
    !text.trim().is_empty() && text.len() <= max && !text.chars().any(char::is_control)
}
fn valid_thumbnail(path: &str) -> bool {
    path.len() <= 512
        && path.starts_with('/')
        && !path.starts_with("//")
        && !path.split('/').any(|v| v == ".." || v == ".")
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/-_.".contains(&b))
}
#[derive(Clone, Debug)]
pub struct Draft {
    pub item: Option<Item>,
    pub tile: Option<TilePos>,
    pub facing: Facing,
    revision: Option<WorldRevision>,
}
impl Default for Draft {
    fn default() -> Self {
        Self {
            item: None,
            tile: None,
            facing: Facing::NORTH,
            revision: None,
        }
    }
}
impl Draft {
    pub fn choose(&mut self, item: Item) {
        self.item = Some(item);
        self.tile = None;
        self.revision = None;
        self.facing = Facing::NORTH;
    }
    pub fn rotate(&mut self) {
        self.facing = match self.facing {
            Facing::NORTH => Facing::EAST,
            Facing::EAST => Facing::SOUTH,
            Facing::SOUTH => Facing::WEST,
            _ => Facing::NORTH,
        };
    }
    pub fn pick(&mut self, pick: &WorldPick, world: &WorldDocument) -> Result<(), &'static str> {
        if !current_revision(pick.revision, world.revision) {
            return Err("The property changed; select the tile again");
        }
        let WorldPickTarget::Tile {
            x,
            y,
            level,
            surface,
        } = pick.target
        else {
            return Err("Choose an empty property tile");
        };
        if x >= world.lot.width
            || y >= world.lot.height
            || level == 0
            || level > world.lot.levels
            || !matches!(
                surface,
                WorldSurface::Terrain | WorldSurface::Floor | WorldSurface::BuildSupport
            )
        {
            return Err("Choose a supported floor or ground tile");
        }
        let x = i16::try_from(x).map_err(|_| "Invalid tile")?;
        let y = i16::try_from(y).map_err(|_| "Invalid tile")?;
        self.tile = Some(TilePos::new(x, y, level));
        self.revision = Some(pick.revision);
        Ok(())
    }
    pub fn request(
        &self,
        c: &Catalogue,
        w: &WorldDocument,
        request_id: u64,
    ) -> Result<ConstructionRequest, &'static str> {
        c.validate(c.binding, w)?;
        if !c.can_build {
            return Err("This account cannot build here");
        }
        if request_id < c.next_request_id || request_id == 0 {
            return Err("Refresh the catalogue before another request");
        }
        let item = self.item.as_ref().ok_or("Choose a catalogue item")?;
        if !c.items.contains(item) {
            return Err("This item changed or was withdrawn; refresh the catalogue");
        }
        let tile = self.tile.ok_or("Tap a property tile first")?;
        if self
            .revision
            .is_none_or(|r| !current_revision(r, w.revision))
        {
            return Err("The property changed; select the tile again");
        }
        let selection = match item.kind {
            ItemKind::Object { catalog_id } => Selection::Purchase {
                catalog_id,
                position: tile.center(),
                facing: self.facing,
            },
            ItemKind::Floor { pattern } => Selection::Floor { tile, pattern },
        };
        Ok(ConstructionRequest {
            binding: c.binding,
            request_id,
            lot_id: c.lot_id,
            authority_epoch: c.authority_epoch,
            architecture_revision: w.revision.architecture_revision,
            permissions_revision: c.permissions_revision,
            account_revision: c.account_revision,
            catalog_revision: c.catalog_revision,
            selections: vec![selection],
        })
    }
    pub fn outline(&self, world: &WorldDocument) -> WorldDraftOutline {
        let Some(tile) = self.tile else {
            return WorldDraftOutline::default();
        };
        if self
            .revision
            .is_none_or(|r| !current_revision(r, world.revision))
        {
            return WorldDraftOutline::default();
        }
        let (Ok(x), Ok(y)) = (u16::try_from(tile.x), u16::try_from(tile.y)) else {
            return WorldDraftOutline::default();
        };
        if x >= world.lot.width
            || y >= world.lot.height
            || tile.level == 0
            || tile.level > world.lot.levels
        {
            return WorldDraftOutline::default();
        }
        let stride = usize::from(world.lot.width) + 1;
        let points: Option<Vec<_>> = [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)]
            .into_iter()
            .map(|(x, y)| {
                let raw = *world
                    .lot
                    .terrain
                    .corners
                    .get(usize::from(y) * stride + usize::from(x))?;
                let height = (f32::from(raw) - f32::from(world.lot.terrain.base_alt)) * 9. / 160.
                    + f32::from(tile.level - 1) * 8.85;
                Some(Vec3::new(
                    f32::from(x) * 3.,
                    height + 0.06,
                    f32::from(y) * 3.,
                ))
            })
            .collect();
        WorldDraftOutline {
            points: points.unwrap_or_default(),
            closed: true,
        }
    }
}
fn current_revision(old: WorldRevision, current: WorldRevision) -> bool {
    old.lot_id.is_some()
        && old.lot_id == current.lot_id
        && old.epoch == current.epoch
        && old.content == current.content
        && old.architecture_revision == current.architecture_revision
        && old.tick <= current.tick
}
