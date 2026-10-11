//! Authored QA prices, objects and RAM-only receipts. NOT a production journal.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use wonderland_game_runtime::live_wire::construction::{
    ConstructionGrant, ConstructionRequest,
    exchange::{self, Command},
    session::ConstructionSession,
};
use wonderland_game_runtime::sim_core::world::build::*;
#[derive(Default)]
pub struct Harness {
    scope: Option<PlayerBinding>,
    session: Option<ConstructionSession>,
    reservations: BTreeMap<u64, (ConstructionRequest, u64)>,
    balance: Option<i64>,
    account_revision: u64,
    next_request: u64,
    pub builds: usize,
    pending: Option<(DurableBuildRequest, i64)>,
}
impl Harness {
    fn binding(input: &Value) -> Result<PlayerBinding> {
        Ok(PlayerBinding {
            source_epoch: input["source_epoch"]
                .as_str()
                .ok_or("source epoch")?
                .parse()?,
            lot_incarnation: input["lot_incarnation"]
                .as_str()
                .ok_or("lot incarnation")?
                .parse()?,
            lot_location: 16777472,
            avatar_id: 42,
        })
    }
    fn grant(&self, actor: EntityRef, binding: PlayerBinding) -> ConstructionGrant {
        let mut authority =
            BuildAuthority::new(actor, PersistentId(42), self.balance.unwrap_or(1000));
        authority.can_build = true;
        authority.permissions_revision = 1;
        authority.account_revision = self.account_revision + 1;
        authority.catalog_revision = 1;
        authority.prices.floor = 7;
        authority.prices.objects.insert(987, 200);
        authority.prices.objects.insert(988, 350);
        for id in 1000..1010 {
            authority
                .prices
                .objects
                .insert(id, 400 + i64::from(id - 1000));
        }
        ConstructionGrant {
            binding,
            principal: support::PRINCIPAL,
            authority,
            floor_patterns: BTreeSet::from([1]),
        }
    }
    fn ensure(
        &mut self,
        runtime: &GameRuntime,
        actor: EntityRef,
        binding: PlayerBinding,
    ) -> Result<ConstructionGrant> {
        if self.scope.is_some_and(|b| b != binding) {
            return Err("Controlled construction binding changed".into());
        }
        let grant = self.grant(actor, binding);
        if self.session.is_none() {
            self.session = Some(ConstructionSession::new(runtime, &grant, 30000)?);
            self.scope = Some(binding);
        }
        Ok(grant)
    }
    pub fn catalogue(
        &mut self,
        runtime: &GameRuntime,
        actor: EntityRef,
        input: &Value,
    ) -> Result<Value> {
        let binding = Self::binding(input)?;
        let grant = self.ensure(runtime, actor, binding)?;
        let s = runtime.sim().state();
        let mut items = vec![
            json!({"kind":{"type":"object","catalog_id":987},"label":"Construction test plinth","category":"Decor","price":200,"thumbnail":"/construction-fixture/plinth.svg"}),
            json!({"kind":{"type":"object","catalog_id":988},"label":"Construction test block","category":"Surfaces","price":350,"thumbnail":"/construction-fixture/block.svg"}),
            json!({"kind":{"type":"floor","pattern":1},"label":"Construction test floor","category":"Floors","price":7,"thumbnail":"/construction-fixture/floor.svg"}),
        ];
        for id in 1000..1010 {
            items.push(json!({"kind":{"type":"object","catalog_id":id},"label":format!("Catalogue test item {}",id-999),"category":"Decor","price":400+id-1000,"thumbnail":"/construction-fixture/plinth.svg"}));
        }
        Ok(
            json!({"version":1,"binding":binding,"lot_id":s.lot_id,"authority_epoch":s.authority_epoch,"content_hash":s.content.content_hash,
            "permissions_revision":grant.authority.permissions_revision,"account_revision":grant.authority.account_revision,"catalog_revision":grant.authority.catalog_revision,
            "balance":grant.authority.balance,"can_build":true,"next_request_id":self.next_request+1,"csrf_token":"fixture-replaced-at-http-boundary",
            "items":items}),
        )
    }
    pub fn exchange(
        &mut self,
        runtime: &mut GameRuntime,
        actor: EntityRef,
        input: &Value,
    ) -> Result<Value> {
        let binding = Self::binding(input)?;
        let grant = self.ensure(runtime, actor, binding)?;
        let bytes = super::unhex(
            input["request"]
                .as_str()
                .ok_or("Missing construction packet")?,
        )?;
        let call = exchange::decode_call(&bytes)?;
        let kind = match call.command {
            Command::Quote(_) => "quote",
            Command::Confirm { .. } => "confirm",
            Command::Cancel { .. } => "cancel",
            Command::Status { .. } => "status",
        };
        let reserved = if let Command::Quote(ref request) = call.command {
            if let Some((old, op)) = self.reservations.get(&request.request_id) {
                if old != request {
                    return Err("Quote changed".into());
                }
                Some(*op)
            } else {
                if self.reservations.len() >= 128 {
                    return Err("Controlled quote budget".into());
                }
                let op = 1000 + self.reservations.len() as u64;
                self.reservations
                    .insert(request.request_id, (request.clone(), op));
                self.next_request = self.next_request.max(request.request_id);
                Some(op)
            }
        } else {
            None
        };
        let handled = exchange::dispatch(
            self.session.as_mut().unwrap(),
            runtime,
            &grant,
            &bytes,
            reserved,
            input["now_ms"].as_u64().unwrap_or(0),
        )?;
        // Route the admitted effect even if response serialization failed. A fixture
        // provider supplies typed completion; the product client never does so.
        let mut transitions = Vec::new();
        if let Some(admitted) = handled.admitted {
            transitions.push(json!({"packet":super::hex(&encode_ticks(&[admitted.frame],WireLimits::default())?)}));
            let durable = admitted
                .outcome
                .events
                .iter()
                .find_map(|e| {
                    if let RuntimeEvent::BuildRequested(r) = e {
                        Some(r.clone())
                    } else {
                        None
                    }
                })
                .ok_or("No actual BuildRequested")?;
            self.builds += 1;
            if self.pending.is_some() {
                return Err("Another controlled completion is pending".into());
            }
            self.pending = Some((durable, grant.authority.balance));
            if input["hold_completion"].as_bool() != Some(true) {
                transitions.push(self.complete(runtime)?);
            }
        }
        let reply = handled.reply?;
        Ok(
            json!({"reply":super::hex(&reply),"transitions":transitions,"kind":kind,"builds":self.builds,"balance":self.balance.unwrap_or(1000)}),
        )
    }
    pub fn balance(&self) -> i64 {
        self.balance.unwrap_or(1000)
    }
    pub fn complete(&mut self, runtime: &mut GameRuntime) -> Result<Value> {
        let (durable, balance) = self
            .pending
            .take()
            .ok_or("No controlled completion is pending")?;
        let confirmation = ServerBuildConfirmation {
            operation: durable.operation,
            actor: durable.actor,
            owner: durable.owner,
            preview_hash: durable.preview_hash,
            charged_cost: durable.cost,
            durable_receipt: durable.operation + 10000,
            status: DurableBuildStatus::Committed,
            created_objects: durable
                .purchases
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    (
                        p.entity,
                        PersistentId(10000 + self.builds as u32 * 100 + i as u32),
                    )
                })
                .collect(),
        };
        let transition = super::tick(runtime, vec![AcceptedCommand::CompleteBuild(confirmation)])?;
        self.balance = Some(balance - durable.cost);
        self.account_revision += 1;
        Ok(transition)
    }
}
pub fn appearance(document: &mut WorldDocument) {
    document.source_counts = None;
    document.provenance.kind = wonderland_game_runtime::world_view::WorldSourceKind::TestFixture;
    use wonderland_game_runtime::world_view::*;
    use wonderland_render_core::{Aabb, AssetKey, Mesh, RgbaImage, Vec2, Vec3, Vertex};
    document.materials.push(WorldMaterial {
        surface: WorldSurface::Floor,
        material: 1,
        style: 0,
        source: "test:construction-floor".into(),
        effective_asset: AssetKey([93; 32]),
        image: RgbaImage {
            width: 1,
            height: 1,
            pixels: vec![[80, 125, 160, 255]],
        },
        uv_scale: Vec2::new(1., 1.),
    });
    for (i, (guid, color)) in [(123, [190, 65, 45, 255]), (124, [45, 90, 190, 255])]
        .into_iter()
        .enumerate()
    {
        let points = [
            (-0.4, 0., -0.4),
            (0.4, 0., -0.4),
            (0.4, 1., -0.4),
            (-0.4, 1., -0.4),
            (-0.4, 0., 0.4),
            (0.4, 0., 0.4),
            (0.4, 1., 0.4),
            (-0.4, 1., 0.4),
        ];
        document.models.push(WorldModel {
            effective_source: AssetKey([61 + i as u8; 32]),
            effective_content: document.revision.content,
            context: ModelContext::Standalone,
            format_version: 2,
            reconstruction_version: 0,
            groups: vec![vec![ModelPart {
                texture: 0,
                mesh: Mesh {
                    vertices: points
                        .into_iter()
                        .map(|(x, y, z)| Vertex {
                            position: Vec3::new(x, y, z),
                            normal: Vec3::Y,
                            uv: Vec2::ZERO,
                            color: [1.; 4],
                        })
                        .collect(),
                    indices: vec![
                        0, 1, 2, 2, 3, 0, 4, 6, 5, 6, 4, 7, 0, 4, 5, 5, 1, 0, 3, 2, 6, 6, 7, 3, 1,
                        5, 6, 6, 2, 1, 0, 3, 7, 7, 4, 0,
                    ],
                },
            }]],
            textures: vec![ModelTexture {
                selector: ModelTextureSelector::Custom { id: 1 },
                effective_asset: AssetKey([71 + i as u8; 32]),
                uv_scale: Vec2::new(1., 1.),
                image: RgbaImage {
                    width: 1,
                    height: 1,
                    pixels: vec![color],
                },
            }],
            bounds: Aabb::new(Vec3::new(-0.4, 0., -0.4), Vec3::new(0.4, 1., 0.4)).unwrap(),
            depth_mask: None,
        });
        // A template, not an existing owned instance. Runtime projection chooses it
        // by the same source GUID only after a real native object is committed.
        document.objects.push(WorldObject {
            source_guid: guid,
            blueprint: None,
            snapshot: None,
            entity: None,
            visual_revision: 0,
            position_tiles: Vec3::ZERO,
            yaw_radians: 0.,
            dynamic_flags: [0; 2],
            room: 0,
            level: 1,
            visible: false,
            selectable: false,
            model: Some(i),
        });
    }
}
