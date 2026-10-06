//! Session and sequence admission for original FreeSO updates.
//! Decoding is not a deterministic VM restore; snapshots are presentation only.
use wonderland_vm_protocol::{
    CommandBody, DecodeLimits, EodMessage, Snapshot, decode_direct_command, decode_tick_list,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveWorldIdentity {
    pub browser_epoch: u64,
    pub source_epoch: u64,
    pub lot_incarnation: u64,
    pub lot_location: u32,
    pub avatar_id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameError {
    WrongSession,
    StaleTick,
    InvalidProtocol,
    WrongLot,
}

#[derive(Debug)]
pub struct FrameUpdate {
    pub generation: u64,
    pub last_tick: Option<u32>,
    pub snapshots: Vec<Snapshot>,
    pub eods: Vec<EodMessage>,
    pub needs_refresh: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceFrameGate {
    identity: Option<LiveWorldIdentity>,
    last_tick: Option<u32>,
    generation: u64,
}

impl SourceFrameGate {
    pub fn reset(&mut self, identity: Option<LiveWorldIdentity>) {
        if self.identity != identity {
            self.identity = identity;
            self.last_tick = None;
            self.generation = 0;
        }
    }
    pub fn identity(&self) -> Option<LiveWorldIdentity> {
        self.identity
    }
    pub fn last_tick(&self) -> Option<u32> {
        self.last_tick
    }
    pub fn admit(
        &mut self,
        identity: LiveWorldIdentity,
        direct: bool,
        bytes: &[u8],
    ) -> Result<FrameUpdate, FrameError> {
        if self.identity != Some(identity) || identity.lot_incarnation == 0 {
            return Err(FrameError::WrongSession);
        }
        let limits = DecodeLimits::default();
        // Decode the entire original payload before publishing any effects.
        let batches = if direct {
            vec![(
                None,
                vec![
                    decode_direct_command(bytes, &limits)
                        .map_err(|_| FrameError::InvalidProtocol)?,
                ],
            )]
        } else {
            let list = decode_tick_list(bytes, &limits).map_err(|_| FrameError::InvalidProtocol)?;
            list.ticks
                .into_iter()
                .map(|tick| {
                    (
                        (!list.immediate_mode).then_some(tick.tick_id),
                        tick.commands,
                    )
                })
                .collect()
        };
        let mut last_tick = self.last_tick;
        let mut snapshots = Vec::new();
        let mut eods = Vec::new();
        let mut needs_refresh = false;
        let mut admitted = false;
        for (tick, commands) in batches {
            let snapshot_only = !commands.is_empty()
                && commands
                    .iter()
                    .all(|command| matches!(&command.body, CommandBody::StateSync { .. }));
            let mut historical = false;
            if let Some(tick) = tick {
                if let Some(previous) = last_tick {
                    let delta = tick.wrapping_sub(previous);
                    historical = delta == 0 || delta >= (1_u32 << 31);
                }
                if snapshot_only {
                    // VMServerDriver.SendState labels a saved state with the
                    // NEXT ordinary TickID. Reject history before that boundary,
                    // but admit its first real tick with the same ID exactly once.
                    let before_snapshot = tick.wrapping_sub(1);
                    if last_tick.is_none_or(|previous| {
                        let delta = before_snapshot.wrapping_sub(previous);
                        delta != 0 && delta < (1_u32 << 31)
                    }) {
                        last_tick = Some(before_snapshot);
                    }
                } else if !historical {
                    last_tick = Some(tick);
                    admitted = true;
                    // Autonomous source simulation also advances on empty ticks.
                    // Until that VM can be restored faithfully, show the last snapshot.
                    needs_refresh = true;
                }
            } else {
                admitted = true;
            }
            for command in commands {
                // VMServerDriver sends its asynchronously serialized LastSync
                // after newer ordinary ticks, followed by TicksSinceSync. A
                // valid StateSync must cross that boundary, while replayed EOD
                // history must never restore old dialog authority. Keep the
                // normal-tick high-water instead of treating a view as a restore.
                if historical && !matches!(&command.body, CommandBody::StateSync { .. }) {
                    continue;
                }
                match command.body {
                    CommandBody::StateSync { snapshot, .. } => {
                        // VMTSOLotState.LotID is LotPersist.location, a packed map
                        // coordinate. It is never the property's database ID.
                        if snapshot.platform.lot_id != identity.lot_location {
                            return Err(FrameError::WrongLot);
                        }
                        snapshots.push(*snapshot);
                        admitted = true;
                        needs_refresh = historical || (tick.is_some() && !snapshot_only);
                    }
                    CommandBody::EodMessage(message) => {
                        if message.actor_uid == identity.avatar_id {
                            eods.push(message);
                        }
                    }
                    _ => {
                        // These source commands affect dialogs/chat or transport.
                        // All other accepted commands can change the world.
                        if !matches!(command.kind, 4 | 13 | 14 | 26 | 34 | 39 | 40 | 41) {
                            needs_refresh = true;
                        }
                    }
                }
            }
        }
        if !admitted {
            return Err(FrameError::StaleTick);
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(FrameError::InvalidProtocol)?;
        self.last_tick = last_tick;
        self.generation = generation;
        Ok(FrameUpdate {
            generation: self.generation,
            last_tick,
            snapshots,
            eods,
            needs_refresh,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn identity(incarnation: u64) -> LiveWorldIdentity {
        LiveWorldIdentity {
            browser_epoch: 2,
            source_epoch: 3,
            lot_incarnation: incarnation,
            lot_location: 0x00f00101,
            avatar_id: 7,
        }
    }
    fn gate() -> SourceFrameGate {
        let mut gate = SourceFrameGate::default();
        gate.reset(Some(identity(1)));
        gate
    }
    fn tick(tick: u32, commands: &[Vec<u8>], immediate: bool) -> Vec<u8> {
        let mut data = vec![u8::from(immediate), 1, 0, 0, 0];
        data.extend(tick.to_le_bytes());
        data.extend(u64::MAX.to_le_bytes());
        data.extend((commands.len() as i32).to_le_bytes());
        for command in commands {
            data.extend(command);
        }
        data
    }
    // Original VMNetEODMessageCmd: type, ActorUID, PluginID, .NET string,
    // binary flag, and .NET text. Independent of the production decoder.
    fn eod(actor: u32) -> Vec<u8> {
        let mut data = vec![18];
        data.extend(actor.to_le_bytes());
        data.extend(0x8b300068u32.to_le_bytes());
        data.extend([
            9, b'e', b'o', b'd', b'_', b'e', b'n', b't', b'e', b'r', 0, 2, b'4', b'2',
        ]);
        data
    }
    #[test]
    fn old_lot_incarnation_cannot_restore_after_same_lot_rejoin() {
        let mut gate = gate();
        gate.admit(identity(1), false, &tick(42, &[], false))
            .unwrap();
        gate.reset(Some(identity(2)));
        let before = gate.clone();
        assert_eq!(
            gate.admit(identity(1), false, &tick(43, &[], false))
                .unwrap_err(),
            FrameError::WrongSession
        );
        assert_eq!(gate, before);
        assert_eq!(gate.last_tick(), None);
    }
    #[test]
    fn logout_prevents_late_original_frames_from_returning() {
        let mut gate = gate();
        gate.reset(None);
        let before = gate.clone();
        assert_eq!(
            gate.admit(identity(1), true, &eod(7)).unwrap_err(),
            FrameError::WrongSession
        );
        assert_eq!(gate, before);
    }
    #[test]
    fn duplicate_tick_is_rejected_without_advancing_the_view_generation() {
        let mut gate = gate();
        let first = gate
            .admit(identity(1), false, &tick(42, &[], false))
            .unwrap();
        assert!(first.needs_refresh);
        assert_eq!(first.last_tick, Some(42));
        let before = gate.clone();
        assert_eq!(
            gate.admit(identity(1), false, &tick(42, &[], false))
                .unwrap_err(),
            FrameError::StaleTick
        );
        assert_eq!(gate, before);
    }
    #[test]
    fn source_tick_rollover_is_ordered_without_accepting_old_packets() {
        let mut gate = gate();
        gate.admit(identity(1), false, &tick(u32::MAX, &[], false))
            .unwrap();
        gate.admit(identity(1), false, &tick(0, &[], false))
            .unwrap();
        assert_eq!(gate.last_tick(), Some(0));
        assert_eq!(
            gate.admit(identity(1), false, &tick(u32::MAX, &[], false))
                .unwrap_err(),
            FrameError::StaleTick
        );
    }
    #[test]
    fn malformed_later_command_cannot_publish_an_earlier_eod() {
        let mut gate = gate();
        let before = gate.clone();
        assert_eq!(
            gate.admit(identity(1), false, &tick(42, &[eod(7), vec![255]], false))
                .unwrap_err(),
            FrameError::InvalidProtocol
        );
        assert_eq!(gate, before);
    }
    #[test]
    fn immediate_eod_does_not_advance_simulation_tick_or_other_players_ui() {
        let mut gate = gate();
        gate.admit(identity(1), false, &tick(42, &[], false))
            .unwrap();
        let result = gate
            .admit(identity(1), false, &tick(0, &[eod(7), eod(8)], true))
            .unwrap();
        assert_eq!(result.last_tick, Some(42));
        assert!(!result.needs_refresh);
        assert_eq!(result.eods.len(), 1);
        assert_eq!(result.eods[0].actor_uid, 7);
    }
    #[test]
    fn direct_source_events_require_exact_complete_payload() {
        let mut gate = gate();
        let result = gate.admit(identity(1), true, &eod(7)).unwrap();
        assert_eq!(result.eods.len(), 1);
        let mut bytes = eod(7);
        bytes.push(0);
        let before = gate.clone();
        assert_eq!(
            gate.admit(identity(1), true, &bytes).unwrap_err(),
            FrameError::InvalidProtocol
        );
        assert_eq!(gate, before);
    }
    #[test]
    fn a_matching_source_snapshot_is_admitted_without_restoring_a_runtime() {
        let bytes =
            include_bytes!("../../../crates/vm-protocol/tests/fixtures/source-v38-state-sync.bin");
        let owner = LiveWorldIdentity {
            lot_location: 55,
            ..identity(1)
        };
        let mut gate = SourceFrameGate::default();
        gate.reset(Some(owner));
        let update = gate.admit(owner, true, bytes).unwrap();
        assert_eq!(update.snapshots.len(), 1);
        assert_eq!(update.snapshots[0].platform.lot_id, 55);
        assert_eq!(update.last_tick, None);
        assert!(!update.needs_refresh);
    }
    #[test]
    fn mismatching_packed_location_rejects_snapshot_and_prior_effects_atomically() {
        let bytes =
            include_bytes!("../../../crates/vm-protocol/tests/fixtures/source-v38-state-sync.bin");
        let mut gate = gate();
        let before = gate.clone();
        assert_eq!(
            gate.admit(
                identity(1),
                false,
                &tick(42, &[eod(7), bytes.to_vec()], false)
            )
            .unwrap_err(),
            FrameError::WrongLot
        );
        assert_eq!(gate, before);
    }
    #[test]
    fn a_tick_after_a_snapshot_requires_refresh_even_in_the_same_packet() {
        let bytes =
            include_bytes!("../../../crates/vm-protocol/tests/fixtures/source-v38-state-sync.bin");
        let owner = LiveWorldIdentity {
            lot_location: 55,
            ..identity(1)
        };
        let mut gate = SourceFrameGate::default();
        gate.reset(Some(owner));
        let mut packet = tick(42, &[bytes.to_vec()], false);
        packet[1..5].copy_from_slice(&2_i32.to_le_bytes());
        packet.extend_from_slice(&tick(43, &[], false)[5..]);
        let update = gate.admit(owner, false, &packet).unwrap();
        assert_eq!(update.snapshots.len(), 1);
        assert_eq!(update.last_tick, Some(43));
        assert!(update.needs_refresh);
    }
    #[test]
    fn original_delayed_snapshot_is_admitted_without_replaying_old_dialogs() {
        let bytes =
            include_bytes!("../../../crates/vm-protocol/tests/fixtures/source-v38-state-sync.bin");
        let owner = LiveWorldIdentity {
            lot_location: 55,
            ..identity(1)
        };
        let mut gate = SourceFrameGate::default();
        gate.reset(Some(owner));
        gate.admit(owner, false, &tick(103, &[], false)).unwrap();
        let update = gate
            .admit(owner, false, &tick(102, &[bytes.to_vec(), eod(7)], false))
            .unwrap();
        assert_eq!(update.snapshots.len(), 1);
        assert_eq!(update.last_tick, Some(103));
        assert!(update.needs_refresh);
        assert!(update.eods.is_empty());
        let before = gate.clone();
        assert_eq!(
            gate.admit(owner, false, &tick(103, &[eod(7)], false))
                .unwrap_err(),
            FrameError::StaleTick
        );
        assert_eq!(gate, before);
        assert_eq!(
            gate.admit(owner, false, &tick(104, &[eod(7)], false))
                .unwrap()
                .eods
                .len(),
            1
        );
    }
    #[test]
    fn snapshot_and_historical_ticks_in_one_packet_preserve_the_newer_cursor() {
        let bytes =
            include_bytes!("../../../crates/vm-protocol/tests/fixtures/source-v38-state-sync.bin");
        let owner = LiveWorldIdentity {
            lot_location: 55,
            ..identity(1)
        };
        let mut gate = SourceFrameGate::default();
        gate.reset(Some(owner));
        gate.admit(owner, false, &tick(103, &[], false)).unwrap();
        let mut packet = tick(102, &[bytes.to_vec()], false);
        packet[1..5].copy_from_slice(&3_i32.to_le_bytes());
        packet.extend_from_slice(&tick(102, &[eod(7)], false)[5..]);
        packet.extend_from_slice(&tick(103, &[eod(7)], false)[5..]);
        let update = gate.admit(owner, false, &packet).unwrap();
        assert_eq!(update.snapshots.len(), 1);
        assert!(update.eods.is_empty());
        assert_eq!(update.last_tick, Some(103));
        assert!(update.needs_refresh);
    }

    #[test]
    fn source_snapshot_wrapper_precedes_the_first_ordinary_tick_with_the_same_id() {
        let command =
            include_bytes!("../../../crates/vm-protocol/tests/fixtures/source-v38-state-sync.bin");
        let owner = LiveWorldIdentity {
            lot_location: 55,
            ..identity(1)
        };
        let mut gate = SourceFrameGate::default();
        gate.reset(Some(owner));
        let snapshot = gate
            .admit(owner, false, &tick(42, &[command.to_vec()], false))
            .unwrap();
        assert!(!snapshot.needs_refresh);
        assert_eq!(
            gate.admit(owner, false, &tick(41, &[eod(7)], false))
                .unwrap_err(),
            FrameError::StaleTick
        );
        let first = gate
            .admit(owner, false, &tick(42, &[eod(7)], false))
            .expect("VMServerDriver tags its snapshot with the next real TickID");
        assert!(first.needs_refresh);
        assert_eq!(first.eods.len(), 1);
        assert_eq!(first.last_tick, Some(42));
        assert_eq!(
            gate.admit(owner, false, &tick(42, &[eod(7)], false))
                .unwrap_err(),
            FrameError::StaleTick
        );
        let cached = gate
            .admit(owner, false, &tick(42, &[command.to_vec()], false))
            .unwrap();
        assert!(
            cached.needs_refresh,
            "The pre-tick snapshot is older than completed tick 42"
        );
        assert!(cached.eods.is_empty());
        assert_eq!(cached.last_tick, Some(42));
    }

    #[test]
    fn snapshot_pre_tick_boundary_preserves_wrapping_source_tick_order() {
        let command =
            include_bytes!("../../../crates/vm-protocol/tests/fixtures/source-v38-state-sync.bin");
        let owner = LiveWorldIdentity {
            lot_location: 55,
            ..identity(1)
        };
        let mut gate = SourceFrameGate::default();
        gate.reset(Some(owner));
        gate.admit(owner, false, &tick(0, &[command.to_vec()], false))
            .unwrap();
        assert_eq!(
            gate.admit(owner, false, &tick(u32::MAX, &[eod(7)], false))
                .unwrap_err(),
            FrameError::StaleTick
        );
        let first = gate
            .admit(owner, false, &tick(0, &[eod(7)], false))
            .unwrap();
        assert_eq!(first.eods.len(), 1);
        assert!(first.needs_refresh);
    }
}
