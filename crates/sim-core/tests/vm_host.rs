#![allow(dead_code)]
use sim_core::ids::{EntityRef, ObjectId};
use sim_core::rng::SimRng;
use sim_core::vm::*;
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Debug, PartialEq)]
pub struct Host {
    pub tick: u32,
    pub rng: SimRng,
    pub entities: BTreeMap<ObjectId, EntityInfo>,
    pub memory: BTreeMap<(ObjectId, EntityField, u16), i16>,
    pub globals: BTreeMap<u16, i16>,
    pub lists: BTreeMap<ObjectId, Vec<i16>>,
    pub operations: Vec<EntityOperation>,
    pub requests: Vec<HostRequest>,
    pub responses: VecDeque<HostResponse>,
    pub random_bounds: Vec<u64>,
    pub rejected_writes: bool,
    pub failed_positions: usize,
    pub relationships: RelationshipBook,
    pub relationship_scale: f32,
    pub behavior_entries: BTreeMap<(ObjectId, u8), BehaviorEntry>,
    pub checks: Vec<RoutineCheck>,
    pub check_results: VecDeque<CheckResult>,
    pub check_positions: VecDeque<Option<(ObjectId, VmPosition)>>,
    pub check_memory: VecDeque<Vec<(MemoryAddress, i16)>>,
    pub check_thread_controls: VecDeque<Option<(bool, u32, PrimitiveExit)>>,
    pub function_states: BTreeMap<EntityRef, FunctionEntityState>,
    pub named_result: Option<BoundRoutine>,
    pub name_lookups: RefCell<Vec<NameLookup>>,
    pub idle_decisions: VecDeque<IdleDecision>,
    pub idle_calls: Vec<(FrameContext, bool, bool, VmMode)>,
    pub interaction_states: BTreeMap<EntityRef, InteractionState>,
    pub available_interactions: bool,
    pub queued: Vec<PushInteractionRequest>,
    pub queue_result: bool,
    pub icons: Vec<(EntityRef, EntityRef)>,
    pub dialog_string: Option<String>,
    pub string_lookups: RefCell<Vec<StringLookup>>,
    pub suit_result: Option<ResolvedSuit>,
    pub suit_lookups: RefCell<Vec<SuitLookup>>,
    pub presentations: Vec<PresentationRequest>,
    pub appearances: Vec<AppearanceOperation>,
    pub observations: Vec<VmThread>,
    pub temp_writes: Vec<(u16, i16)>,
    pub temp_xl_writes: Vec<(u16, i32)>,
    pub entity_temps_alias: bool,
    pub thread_control: Option<(bool, u32, PrimitiveExit)>,
    pub control_on_request: Option<(bool, u32, PrimitiveExit)>,
    pub reset_requested: bool,
    pub interrupt_requested: bool,
    pub fire: FireState,
    pub fire_percent_writes: Vec<i32>,
    pub pool: bool,
    pub failed_creations: usize,
    pub create_out_of_world: bool,
    pub new_object_burnable: bool,
    pub family_budget: Option<i32>,
    pub budget_writes: Vec<i32>,
    pub ts1_inventory: Ts1InventoryBook,
    pub autonomy: Option<sim_core::avatars::autonomy::AutonomyContext>,
    pub offers: Option<AutonomyOffers>,
    pub offer_requests: Vec<AutonomyOfferRequest>,
    pub autonomy_enqueued: Vec<AutonomyEnqueue>,
    pub autonomy_enqueue_result: bool,
    pub push_results: VecDeque<Option<RoutineCall>>,
    pub push_attempts: Vec<(EntityRef, FrameContext)>,
}
pub fn reference(id: i16) -> EntityRef {
    EntityRef {
        object_id: ObjectId(id),
        generation: 1,
    }
}
impl Host {
    pub fn new() -> Self {
        let mut host = Self {
            tick: 1,
            rng: SimRng::new(1),
            entities: BTreeMap::new(),
            memory: BTreeMap::new(),
            globals: BTreeMap::new(),
            lists: BTreeMap::new(),
            operations: Vec::new(),
            requests: Vec::new(),
            responses: VecDeque::new(),
            random_bounds: Vec::new(),
            rejected_writes: false,
            failed_positions: 0,
            relationships: RelationshipBook::default(),
            relationship_scale: 1.0,
            behavior_entries: BTreeMap::new(),
            checks: Vec::new(),
            check_results: VecDeque::new(),
            check_positions: VecDeque::new(),
            check_memory: VecDeque::new(),
            check_thread_controls: VecDeque::new(),
            function_states: BTreeMap::new(),
            named_result: None,
            name_lookups: RefCell::new(Vec::new()),
            idle_decisions: VecDeque::new(),
            idle_calls: Vec::new(),
            interaction_states: BTreeMap::new(),
            available_interactions: true,
            queued: Vec::new(),
            queue_result: true,
            icons: Vec::new(),
            dialog_string: None,
            string_lookups: RefCell::new(Vec::new()),
            suit_result: None,
            suit_lookups: RefCell::new(Vec::new()),
            presentations: Vec::new(),
            appearances: Vec::new(),
            observations: Vec::new(),
            temp_writes: Vec::new(),
            temp_xl_writes: Vec::new(),
            entity_temps_alias: true,
            thread_control: None,
            control_on_request: None,
            reset_requested: false,
            interrupt_requested: false,
            fire: FireState {
                enabled: true,
                percent: 20000,
                width: 64,
                height: 64,
            },
            fire_percent_writes: Vec::new(),
            pool: false,
            failed_creations: 0,
            create_out_of_world: false,
            new_object_burnable: false,
            family_budget: None,
            budget_writes: Vec::new(),
            ts1_inventory: Ts1InventoryBook::default(),
            autonomy: None,
            offers: None,
            offer_requests: Vec::new(),
            autonomy_enqueued: Vec::new(),
            autonomy_enqueue_result: true,
            push_results: VecDeque::new(),
            push_attempts: Vec::new(),
        };
        host.add(1, 0x1234, 0, 0);
        host.add(2, 0x5678, 16, 0);
        host
    }
    pub fn add(&mut self, id: i16, guid: u32, x: i16, y: i16) {
        self.entities.insert(
            ObjectId(id),
            EntityInfo {
                reference: reference(id),
                guid,
                master_guid: None,
                semiglobal: Some(42),
                persistent_id: id as u32 + 0x12340000,
                is_avatar: id == 1,
                dead: false,
                category: 0,
                family: 0,
                position: VmPosition { x, y, level: 1 },
                direction: 0,
                level_offset: 0,
                base_object: ObjectId(id),
                multi_tile: false,
                group: vec![ObjectId(id)],
            },
        );
        self.lists.insert(ObjectId(id), Vec::new());
    }
    pub fn commit_deletions(&mut self) {
        let deleted = self
            .operations
            .iter()
            .filter_map(|operation| {
                if let EntityOperation::Delete { target, .. } = operation {
                    Some(target.object_id)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for id in deleted {
            self.entities.remove(&id);
        }
    }
}
impl VmHost for Host {
    fn ts1_family_budget(&self) -> Result<Option<i32>, VmFault> {
        Ok(self.family_budget)
    }
    fn set_ts1_family_budget(&mut self, budget: i32) -> Result<(), VmFault> {
        self.family_budget = Some(budget);
        self.budget_writes.push(budget);
        Ok(())
    }
    fn ts1_inventory_read(&self, neighbor: i16) -> Result<Option<Vec<Ts1InventoryItem>>, VmFault> {
        Ok(self.ts1_inventory.read(neighbor))
    }
    fn ts1_inventory_write(
        &mut self,
        neighbor: i16,
        items: Vec<Ts1InventoryItem>,
    ) -> Result<(), VmFault> {
        self.ts1_inventory.write(neighbor, items)
    }
    fn autonomy_context(
        &self,
        _caller: EntityRef,
    ) -> Result<sim_core::avatars::autonomy::AutonomyContext, VmFault> {
        self.autonomy
            .clone()
            .ok_or_else(|| VmFault::HostUnsupported("test autonomy context".into()))
    }
    fn autonomy_offers(
        &mut self,
        request: AutonomyOfferRequest,
    ) -> Result<AutonomyOffers, VmFault> {
        self.offer_requests.push(request);
        self.offers
            .clone()
            .ok_or_else(|| VmFault::HostUnsupported("test autonomy offers".into()))
    }
    fn enqueue_autonomy(&mut self, request: AutonomyEnqueue) -> Result<bool, VmFault> {
        self.autonomy_enqueued.push(request);
        Ok(self.autonomy_enqueue_result)
    }
    fn attempt_push(
        &mut self,
        owner: EntityRef,
        context: &FrameContext,
    ) -> Result<Option<RoutineCall>, VmFault> {
        self.push_attempts.push((owner, context.clone()));
        Ok(self.push_results.pop_front().flatten())
    }
    fn fire_state(&self) -> Result<FireState, VmFault> {
        Ok(self.fire)
    }
    fn set_fire_percent(&mut self, percent: i32) -> Result<(), VmFault> {
        self.fire.percent = percent;
        self.fire_percent_writes.push(percent);
        Ok(())
    }
    fn room_is_pool(&self, _entity: EntityRef) -> Result<bool, VmFault> {
        Ok(self.pool)
    }
    fn observe_thread(&mut self, thread: &VmThread) {
        self.observations.push(thread.clone());
    }
    fn take_temp_writes(&mut self, _owner: EntityRef) -> Vec<(u16, i16)> {
        std::mem::take(&mut self.temp_writes)
    }
    fn take_temp_xl_writes(&mut self, _owner: EntityRef) -> Vec<(u16, i32)> {
        std::mem::take(&mut self.temp_xl_writes)
    }
    fn entity_temps_alias(&self, _owner: EntityRef) -> bool {
        self.entity_temps_alias
    }
    fn take_thread_control(&mut self, _owner: EntityRef) -> Option<(bool, u32, PrimitiveExit)> {
        self.thread_control.take()
    }
    fn take_reset_request(&mut self, _owner: EntityRef) -> bool {
        std::mem::take(&mut self.reset_requested)
    }
    fn take_interrupt_request(&mut self, _owner: EntityRef) -> bool {
        std::mem::take(&mut self.interrupt_requested)
    }
    fn relationship_read(&self, key: RelationshipKey) -> Result<Option<Vec<i16>>, VmFault> {
        Ok(self.relationships.read(key))
    }
    fn relationship_write(
        &mut self,
        key: RelationshipKey,
        values: Vec<i16>,
    ) -> Result<(), VmFault> {
        self.relationships.write(key, values)
    }
    fn relationship_mark(&mut self, key: RelationshipKey, dirty: bool) -> Result<(), VmFault> {
        self.relationships.mark(key, dirty)
    }
    fn relationship_multiplier(&self) -> Result<f32, VmFault> {
        Ok(self.relationship_scale)
    }
    fn behavior_entry(&self, entity: EntityRef, entry: u8) -> Result<BehaviorEntry, VmFault> {
        self.entity_info(entity)?;
        Ok(self
            .behavior_entries
            .get(&(entity.object_id, entry))
            .cloned()
            .unwrap_or_default())
    }
    fn evaluate_check(&mut self, request: RoutineCheck) -> Result<CheckResult, VmFault> {
        let fallback = CheckResult {
            accepted: true,
            exit: PrimitiveExit::ReturnTrue,
            aborting: false,
            temps: request.temps,
            temp_xl: request.temp_xl,
            copy_back: false,
            thread_control: None,
        };
        self.checks.push(request);
        if let Some(Some((id, position))) = self.check_positions.pop_front() {
            self.entities.get_mut(&id).unwrap().position = position;
        }
        if let Some(writes) = self.check_memory.pop_front() {
            for (address, value) in writes {
                self.write_memory(&address, value)?;
            }
        }
        if let Some(control) = self.check_thread_controls.pop_front() {
            self.thread_control = control;
        }
        Ok(self.check_results.pop_front().unwrap_or(fallback))
    }
    fn named_tree(&self, lookup: NameLookup) -> Result<Option<BoundRoutine>, VmFault> {
        self.name_lookups.borrow_mut().push(lookup);
        Ok(self.named_result.clone())
    }
    fn function_status(&self, entity: EntityRef) -> Result<FunctionEntityState, VmFault> {
        self.entity_info(entity)?;
        Ok(self
            .function_states
            .get(&entity)
            .copied()
            .unwrap_or_default())
    }
    fn idle_for_input(
        &mut self,
        context: &FrameContext,
        allow_push: bool,
        action_tree: bool,
        mode: VmMode,
    ) -> Result<IdleDecision, VmFault> {
        self.idle_calls
            .push((context.clone(), allow_push, action_tree, mode));
        Ok(self
            .idle_decisions
            .pop_front()
            .unwrap_or(IdleDecision::Quiet))
    }
    fn interaction_state(&self, entity: EntityRef) -> Result<InteractionState, VmFault> {
        Ok(self
            .interaction_states
            .get(&entity)
            .copied()
            .unwrap_or_default())
    }
    fn interaction_available(
        &self,
        _source: EntityRef,
        _target: EntityRef,
        _interaction: u8,
    ) -> Result<bool, VmFault> {
        Ok(self.available_interactions)
    }
    fn push_interaction(&mut self, request: PushInteractionRequest) -> Result<bool, VmFault> {
        self.queued.push(request);
        Ok(self.queue_result)
    }
    fn change_interaction_icon(
        &mut self,
        caller: EntityRef,
        icon: EntityRef,
    ) -> Result<(), VmFault> {
        self.icons.push((caller, icon));
        Ok(())
    }
    fn resolve_dialog_string(&self, lookup: StringLookup) -> Result<Option<String>, VmFault> {
        self.string_lookups.borrow_mut().push(lookup);
        Ok(self.dialog_string.clone())
    }
    fn presentation(&mut self, request: PresentationRequest) -> Result<(), VmFault> {
        self.presentations.push(request);
        Ok(())
    }
    fn resolve_suit(&self, lookup: SuitLookup) -> Result<Option<ResolvedSuit>, VmFault> {
        self.suit_lookups.borrow_mut().push(lookup);
        Ok(self.suit_result.clone())
    }
    fn apply_appearance(&mut self, operation: AppearanceOperation) -> Result<(), VmFault> {
        self.appearances.push(operation);
        Ok(())
    }
    fn current_tick(&self) -> u32 {
        self.tick
    }
    fn next_random(&mut self, bound: u64) -> u64 {
        self.random_bounds.push(bound);
        self.rng.next(bound)
    }
    fn resolve_entity(&self, id: ObjectId) -> Option<EntityRef> {
        self.entities.get(&id).map(|e| e.reference)
    }
    fn entity_info(&self, entity: EntityRef) -> Result<EntityInfo, VmFault> {
        let info = self
            .entities
            .get(&entity.object_id)
            .ok_or(VmFault::MissingEntity(entity.object_id))?;
        if info.reference != entity {
            return Err(VmFault::StaleEntity(entity));
        }
        Ok(info.clone())
    }
    fn entity_ids(&self) -> Result<Vec<ObjectId>, VmFault> {
        Ok(self.entities.keys().rev().copied().collect())
    }
    fn read_memory(&self, address: &MemoryAddress) -> Result<i16, VmFault> {
        match address {
            MemoryAddress::Entity {
                entity,
                field,
                index,
            } => {
                self.entity_info(*entity)?;
                if *field == EntityField::ObjectData && *index == 11 {
                    return Ok(entity.object_id.0);
                }
                if *field == EntityField::ObjectData && *index == 1 {
                    return Ok(*self
                        .memory
                        .get(&(entity.object_id, *field, *index))
                        .unwrap_or(&(self.entity_info(*entity)?.direction as i16)));
                }
                Ok(*self
                    .memory
                    .get(&(entity.object_id, *field, *index))
                    .unwrap_or(&0))
            }
            MemoryAddress::Global(id) => Ok(*self.globals.get(id).unwrap_or(&0)),
            MemoryAddress::Tuning {
                table_id, key_id, ..
            } => Ok(table_id.wrapping_add(*key_id) as i16),
            MemoryAddress::TreeAdvertisement { .. } => Ok(0),
            _ => Err(VmFault::HostUnsupported(format!("test memory {address:?}"))),
        }
    }
    fn write_memory(&mut self, address: &MemoryAddress, value: i16) -> Result<bool, VmFault> {
        if self.rejected_writes {
            return Ok(false);
        }
        match address {
            MemoryAddress::Entity {
                entity,
                field,
                index,
            } => {
                self.entity_info(*entity)?;
                if *field == EntityField::Temp
                    && self.observations.last().map(|thread| thread.owner) == Some(*entity)
                {
                    self.temp_writes.push((*index, value));
                }
                self.memory
                    .insert((entity.object_id, *field, *index), value);
            }
            MemoryAddress::Global(id) => {
                self.globals.insert(*id, value);
            }
            _ => return Err(VmFault::HostUnsupported("test write".into())),
        }
        Ok(true)
    }
    fn read_list(&self, entity: EntityRef) -> Result<Vec<i16>, VmFault> {
        self.entity_info(entity)?;
        Ok(self
            .lists
            .get(&entity.object_id)
            .cloned()
            .unwrap_or_default())
    }
    fn replace_list(&mut self, entity: EntityRef, values: Vec<i16>) -> Result<(), VmFault> {
        self.entity_info(entity)?;
        self.lists.insert(entity.object_id, values);
        Ok(())
    }
    fn entity_operation(
        &mut self,
        operation: EntityOperation,
    ) -> Result<EntityOperationResult, VmFault> {
        self.operations.push(operation.clone());
        match operation {
            EntityOperation::ChangePosition {
                target,
                position,
                direction,
            } => {
                if self.failed_positions > 0 {
                    self.failed_positions -= 1;
                    return Ok(EntityOperationResult::Bool(false));
                }
                let info = self.entities.get_mut(&target.object_id).unwrap();
                info.position = position;
                info.direction = direction;
                Ok(EntityOperationResult::Bool(true))
            }
            EntityOperation::Create {
                guid,
                position,
                direction,
                ..
            } => {
                if self.failed_creations > 0 {
                    self.failed_creations -= 1;
                    return Ok(EntityOperationResult::Created(None));
                }
                let id = self.entities.keys().next_back().unwrap().0 + 1;
                self.add(id, guid, position.x, position.y);
                let info = self.entities.get_mut(&ObjectId(id)).unwrap();
                info.position = position;
                info.direction = direction;
                if self.create_out_of_world {
                    info.position = VmPosition {
                        x: i16::MIN,
                        y: i16::MIN,
                        level: 1,
                    };
                }
                if self.new_object_burnable {
                    self.memory
                        .insert((ObjectId(id), EntityField::ObjectData, 40), 32);
                }
                Ok(EntityOperationResult::Created(Some(reference(id))))
            }
            EntityOperation::PlaceInSlot {
                container,
                object,
                slot,
                ..
            } => {
                self.memory.insert(
                    (container.object_id, EntityField::Slot, slot as u16),
                    object.map(|o| o.object_id.0).unwrap_or(0),
                );
                if let Some(object) = object {
                    self.memory.insert(
                        (object.object_id, EntityField::ObjectData, 2),
                        container.object_id.0,
                    );
                }
                Ok(EntityOperationResult::Bool(true))
            }
            _ => Ok(EntityOperationResult::Bool(true)),
        }
    }
    fn request(&mut self, request: HostRequest) -> Result<HostResponse, VmFault> {
        self.requests.push(request);
        if let Some(control) = self.control_on_request.take() {
            self.thread_control = Some(control);
        }
        Ok(self
            .responses
            .pop_front()
            .unwrap_or(HostResponse::Complete(PrimitiveExit::GotoTrue)))
    }
    fn show_money_headline(&mut self, _caller: EntityRef, _amount: i32) -> Result<(), VmFault> {
        Ok(())
    }
}
pub fn setup(instructions: Vec<VmInstruction>) -> (RoutineStore, VmThread, Host) {
    let mut store = RoutineStore::new();
    let key = RoutineKey {
        scope: RoutineScope::Global,
        id: 256,
    };
    store
        .insert(key, VmRoutine::new(256, 8, 4, instructions).unwrap())
        .unwrap();
    let mut thread = VmThread::new(reference(1), VmMode::Tso);
    thread
        .push_entry(
            &store,
            key,
            FrameContext::for_entity(reference(1), 0x1234),
            vec![0; 4],
        )
        .unwrap();
    (store, thread, Host::new())
}
pub fn instruction(opcode: u16) -> VmInstruction {
    VmInstruction::new(opcode, 254, 255, [0; 8])
}
