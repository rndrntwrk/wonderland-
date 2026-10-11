//! Source StateSync refresh. Presentation generations expire picks; they do not
//! acknowledge transactions or pretend to be server command revisions.
use crate::*;
use wonderland_vm_protocol::snapshot::{Appearance, EntityPlatform, Snapshot};
impl AuthoringState {
    pub fn observe_snapshot(
        &mut self,
        actor: SourceActorLot,
        source: &Snapshot,
        presentation_generation: u64,
    ) -> Result<(), AuthoringError> {
        if source.platform.lot_id != actor.location || presentation_generation == 0 {
            return Err(AuthoringError::Stale);
        }
        let selected = source
            .entities
            .iter()
            .find(|e| {
                e.persist_id == actor.avatar_id
                    && matches!(e.platform, EntityPlatform::Avatar { .. })
            })
            .ok_or(AuthoringError::Missing("source actor"))?;
        let EntityPlatform::Avatar {
            budget,
            permissions,
            ..
        } = selected.platform
        else {
            unreachable!()
        };
        if permissions > 4 {
            return Err(AuthoringError::Invalid("source permission"));
        }
        let same = self.snapshot.as_ref().is_some_and(|s| s.actor == actor);
        if same
            && self
                .presentation_generation
                .is_some_and(|old| presentation_generation <= old)
        {
            return Err(AuthoringError::Stale);
        }
        let mut snapshot = self
            .snapshot
            .as_ref()
            .filter(|s| s.actor == actor)
            .cloned()
            .unwrap_or_else(|| AuthoringSnapshot {
                actor: actor.clone(),
                revision: 0,
                permission: None,
                community: false,
                bounds: None,
                budget: None,
                can_place_user: false,
                can_place_donated: false,
                build_resources: vec![],
                outfits_loaded: false,
                catalog: vec![],
                inventory: vec![],
                objects: vec![],
                outfits: vec![],
                defaults: Default::default(),
                eod: None,
            });
        snapshot.permission = Some(permissions);
        snapshot.budget = Some(budget);
        snapshot.community = source.platform.category == 11;
        let a = &source.context.architecture;
        snapshot.bounds = Some(LotBounds {
            width: a.width,
            height: a.height,
            levels: a.stories,
        });
        snapshot.objects = source
            .entities
            .iter()
            .map(|e| {
                let (owner_id, donated, is_avatar) = match &e.platform {
                    EntityPlatform::Object {
                        owner_id, flags, ..
                    } => (*owner_id, flags & 1 != 0, false),
                    EntityPlatform::Avatar { .. } => (e.persist_id, false, true),
                };
                let (direction, transaction_incomplete) = match e.appearance {
                    Appearance::Object {
                        direction,
                        disabled,
                    } => (direction, disabled & 1 != 0),
                    _ => (1, false),
                };
                let name = source
                    .multitile_groups
                    .iter()
                    .find(|g| g.objects.contains(&e.object_id))
                    .map(|g| g.name.clone())
                    .unwrap_or_default();
                SourceObject {
                    entity: EntityIdentity {
                        object_id: e.object_id,
                        incarnation: presentation_generation,
                    },
                    persist_id: e.persist_id,
                    guid: e.guid,
                    name,
                    owner_id,
                    donated,
                    is_avatar,
                    movable: None,
                    transaction_incomplete,
                    placement: if is_avatar {
                        None
                    } else {
                        Some(Placement {
                            x: e.position.x,
                            y: e.position.y,
                            level: e.position.level,
                            direction,
                        })
                    },
                }
            })
            .collect();
        if let Some(eod) = snapshot.eod.as_mut() {
            eod.object_owner_id = eod.object_pid.and_then(|pid| {
                snapshot
                    .objects
                    .iter()
                    .find(|o| o.persist_id == pid && !o.is_avatar)
                    .map(|o| o.owner_id)
            });
        }
        if let Appearance::Avatar(avatar) = &selected.appearance {
            for (category, suit) in [0, 2, 5].into_iter().zip(&avatar.default_suits) {
                snapshot.defaults.insert(category, suit.id.to_string());
            }
        }
        snapshot.revision = snapshot
            .revision
            .checked_add(1)
            .ok_or(AuthoringError::Invalid("source observation revision"))?;
        snapshot.validate()?;
        if same
            && self
                .pending
                .as_ref()
                .is_some_and(|r| matches!(r.wire, AuthoringWire::LotCommand { .. }))
        {
            self.status=OperationState::Unknown("The world refreshed before this edit was confirmed. Check the item's current state.".into());
        }
        if self.draft.as_ref().is_some_and(|d| {
            matches!(
                d,
                AuthoringIntent::Buy { .. }
                    | AuthoringIntent::PlaceInventory { .. }
                    | AuthoringIntent::Move { .. }
                    | AuthoringIntent::Delete { .. }
                    | AuthoringIntent::SendToInventory { .. }
                    | AuthoringIntent::Architecture { .. }
                    | AuthoringIntent::SetRoof { .. }
            )
        }) {
            self.draft = None;
        }
        self.install(snapshot)?;
        self.diagonal_floor_tiles = Some(
            a.walls
                .iter()
                .enumerate()
                .flat_map(|(level, walls)| {
                    walls
                        .iter()
                        .enumerate()
                        .filter(|(_, w)| w.segments & 48 != 0)
                        .map(move |(cell, _)| {
                            (
                                (cell % usize::from(a.width)) as u16,
                                (cell / usize::from(a.width)) as u16,
                                (level + 1) as u8,
                            )
                        })
                })
                .collect(),
        );
        self.presentation_generation = Some(presentation_generation);
        Ok(())
    }
}
