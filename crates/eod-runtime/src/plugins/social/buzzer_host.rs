// This Source Code Form is subject to the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
use super::{buzzer_player::*, util::*, *};

const DISABLED: u8 = 0;
const READY: u8 = 1;
const ENGAGED: u8 = 2;
const LOCKED: u8 = 3;
const EXPIRED: u8 = 4;

#[derive(Clone)]
struct Contestant {
    group: InstanceId,
    avatar_id: u32,
    avatar_object: i16,
    name: String,
    score: i16,
    enabled: bool,
    owner: Option<InstanceId>,
    visited: u64,
}
#[derive(Clone)]
struct Options {
    auto_enable: bool,
    auto_disable: bool,
    auto_deduct: bool,
    score: i16,
    answer_time: i16,
    buzzer_time: i16,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            auto_enable: true,
            auto_disable: true,
            auto_deduct: false,
            score: 100,
            answer_time: 20,
            buzzer_time: 10,
        }
    }
}
#[derive(Clone)]
pub(super) struct Host {
    own_group: Option<InstanceId>,
    joined: bool,
    closing: bool,
    contestants: Vec<Contestant>,
    slots: [Option<InstanceId>; 4],
    phase: u8,
    tock: u8,
    timer: i16,
    engaged_ticks: u8,
    answering: Option<u8>,
    winner: Option<(InstanceId, String)>,
    options: Options,
    revision: u64,
}
impl Host {
    pub(super) fn required_peers(&self) -> Vec<InstanceId> {
        self.slots.iter().flatten().copied().collect()
    }

    pub(super) fn new() -> Self {
        Self {
            own_group: None,
            joined: false,
            closing: false,
            contestants: Vec::new(),
            slots: [None; 4],
            phase: DISABLED,
            tock: 0,
            timer: 0,
            engaged_ticks: 0,
            answering: None,
            winner: None,
            options: Options::default(),
            revision: 1,
        }
    }
    pub(super) fn start(&self) -> Actions {
        let mut a = Actions::default();
        a.peer(BUZZER_PLAYER, signal(SCAN, Vec::new(), Vec::new()));
        a
    }
    fn contestant(&self, slot: usize) -> Option<&Contestant> {
        let group = self.slots.get(slot).copied().flatten()?;
        self.contestants.iter().find(|p| p.group == group)
    }
    fn contestant_mut(&mut self, slot: usize) -> Option<&mut Contestant> {
        let group = self.slots.get(slot).copied().flatten()?;
        self.contestants.iter_mut().find(|p| p.group == group)
    }
    fn control(a: &mut Actions, player: InstanceId, command: i64, values: &[i64], bytes: Vec<u8>) {
        let mut numbers = Vec::with_capacity(values.len() + 2);
        numbers.push(target_id(player));
        numbers.push(command);
        numbers.extend_from_slice(values);
        a.peer(BUZZER_PLAYER, signal(CONTROL, numbers, bytes));
    }
    fn roster(&self, a: &mut Actions) {
        if !self.joined || self.closing {
            return;
        }
        let mut data = Vec::with_capacity(12);
        let mut any = false;
        for slot in 0..4 {
            if let Some(player) = self.contestant(slot) {
                any = true;
                data.extend([
                    player.avatar_object.to_string(),
                    player.score.to_string(),
                    u8::from(player.enabled).to_string(),
                ]);
            } else {
                data.extend(["0".to_owned(), "0".to_owned(), "0".to_owned()]);
            }
        }
        a.binary(
            Target::All,
            if self.phase == READY {
                "Buzzer_Host_Live_Roster"
            } else {
                "Buzzer_Host_Roster"
            },
            strings(data.iter().map(String::as_str)),
        );
        a.object(Target::Controller, 5, vec![i16::from(any)]);
    }
    fn no_changes(a: &mut Actions) {
        a.binary(Target::All, "Buzzer_Host_Error", vec![0; 32]);
    }
    fn set_enabled(&mut self, slot: usize, value: bool, a: &mut Actions) {
        if let Some(player) = self.contestant_mut(slot) {
            player.enabled = value;
            Self::control(a, player.group, 3, &[i64::from(value)], Vec::new());
        }
    }
    fn broadcast(
        &self,
        a: &mut Actions,
        command: i64,
        value: i64,
        active_only: bool,
        skip: Option<usize>,
    ) {
        for slot in 0..4 {
            if skip == Some(slot) {
                continue;
            }
            if let Some(player) = self.contestant(slot)
                && (!active_only || player.enabled)
            {
                Self::control(a, player.group, command, &[value], Vec::new());
            }
        }
    }
    fn set_timer(&mut self, value: i16, a: &mut Actions) {
        self.timer = value;
        self.broadcast(a, 12, i64::from(value), false, None);
        a.text(Target::All, "BuzzerEOD_Timer", value.to_string());
    }
    fn change_phase(&mut self, phase: u8, a: &mut Actions) {
        match phase {
            READY => {
                self.set_timer(self.options.buzzer_time, a);
                self.broadcast(a, 11, 1, true, None);
                self.tock = 0;
                self.answering = None;
                self.engaged_ticks = 0;
            }
            ENGAGED => {
                self.set_timer(self.options.answer_time, a);
                self.engaged_ticks = 30;
                self.tock = 0;
            }
            EXPIRED => a.text(Target::All, "Buzzer_Host_Tip", "38".to_owned()),
            DISABLED => {
                self.answering = None;
                self.broadcast(a, 11, 0, false, None);
                self.tock = 0;
                self.engaged_ticks = 0;
            }
            LOCKED => self.broadcast(a, 8, 0, true, self.answering.map(usize::from)),
            _ => {}
        }
        self.phase = phase;
    }
    fn fill(&mut self, index: usize, via_search: bool, a: &mut Actions) {
        if !self.joined || self.closing {
            return;
        }
        if let Some(current) = self.slots[index]
            && let Some(player) = self.contestants.iter_mut().find(|p| p.group == current)
        {
            player.visited = self.revision;
        }
        let available: Vec<usize> = self
            .contestants
            .iter()
            .enumerate()
            .filter(|(_, p)| p.owner.is_none() && !self.slots.contains(&Some(p.group)))
            .map(|(index, _)| index)
            .collect();
        if available.is_empty() {
            if via_search {
                a.text(Target::All, "Buzzer_Host_Error", "31".to_owned());
            }
        } else {
            let selected = available
                .iter()
                .copied()
                .find(|index| self.contestants[*index].visited < self.revision)
                .unwrap_or(available[0]);
            if !available
                .iter()
                .any(|index| self.contestants[*index].visited < self.revision)
            {
                for index in available {
                    self.contestants[index].visited = 0;
                }
            }
            if let Some(old) = self.slots[index] {
                Self::control(a, old, 2, &[], Vec::new());
                if let Some(player) = self.contestants.iter_mut().find(|p| p.group == old) {
                    player.owner = None;
                }
            }
            let player = &mut self.contestants[selected];
            self.slots[index] = Some(player.group);
            // Reservation is private and provisional until the peer confirms
            // its controller-scoped claim in this same host transaction.
            player.owner = self.own_group;
            Self::control(a, player.group, 1, &[], Vec::new());
        }
        if via_search {
            self.roster(a);
        }
    }
    fn fill_empty(&mut self, a: &mut Actions) {
        for index in 0..4 {
            if self.slots[index].is_none() {
                self.fill(index, false, a);
            }
        }
    }
    pub(super) fn join(&mut self, member: Member, roster: &Roster) -> Result<Actions, Error> {
        if !valid_single(roster) || self.joined {
            return Err(Error::InvalidPluginInput);
        }
        self.joined = true;
        self.closing = false;
        self.phase = DISABLED;
        self.options = Options::default();
        let mut a = Actions::default();
        self.fill_empty(&mut a);
        self.roster(&mut a);
        a.binary(Target::Member(member.seat), "BuzzerEOD_Init", vec![1]);
        append(&mut a, self.start());
        Ok(a)
    }
    pub(super) fn rebind(&self, member: Member) -> Actions {
        let mut a = Actions::default();
        let target = Target::Member(member.seat);
        a.binary(target, "BuzzerEOD_Init", vec![1]);
        self.roster(&mut a);
        for (event, value) in [
            ("Buzzer_Host_B_Deduct", self.options.auto_deduct),
            ("Buzzer_Host_B_Disable", self.options.auto_disable),
            ("Buzzer_Host_B_Enable", self.options.auto_enable),
            ("Buzzer_Host_B_ToggleMaster", self.phase == READY),
        ] {
            a.binary(target, event, vec![u8::from(value)]);
        }
        for (event, value) in [
            ("Buzzer_Host_B_AnswerTime", self.options.answer_time),
            ("Buzzer_Host_B_BuzzerTime", self.options.buzzer_time),
            ("Buzzer_Host_B_GlobalScore", self.options.score),
        ] {
            a.binary(target, event, value.to_le_bytes().to_vec());
        }
        if let Some(answering) = self.answering {
            a.binary(
                target,
                "BuzzerEOD_Buzzed",
                i32::from(answering).to_le_bytes().to_vec(),
            );
        }
        if let Some((_, name)) = &self.winner {
            a.text(target, "Buzzer_Player_Win", name.clone());
        }
        a.text(target, "BuzzerEOD_Timer", self.timer.to_string());
        a
    }
    pub(super) fn tick(&mut self) -> Actions {
        let mut a = Actions::default();
        if !self.joined || self.closing {
            return a;
        }
        if matches!(self.phase, READY | ENGAGED | LOCKED) {
            self.tock += 1;
            if self.tock >= 30 {
                self.tock = 0;
                self.set_timer((self.timer - 1).max(0), &mut a);
                if self.timer == 0 && self.phase != ENGAGED {
                    self.change_phase(EXPIRED, &mut a);
                }
            }
            if self.phase == ENGAGED {
                self.engaged_ticks -= 1;
                if self.engaged_ticks == 0 {
                    self.change_phase(LOCKED, &mut a);
                }
            }
        }
        a
    }
    fn judge(&mut self, slot: usize, correct: bool, a: &mut Actions) {
        if self.phase == READY {
            Self::no_changes(a);
            self.roster(a);
            return;
        }
        if let Some(player) = self.contestant(slot).cloned() {
            let points = if correct || self.options.auto_deduct {
                self.options.score
            } else {
                0
            };
            Self::control(
                a,
                player.group,
                5,
                &[i64::from(correct), i64::from(points)],
                Vec::new(),
            );
            if let Some(known) = self.contestant_mut(slot) {
                known.score = if correct {
                    (i32::from(known.score) + i32::from(points)).min(9999) as i16
                } else {
                    (i32::from(known.score) - i32::from(points)).max(0) as i16
                };
            }
            if correct && self.options.auto_enable {
                // Preserve the original loop boundary: slot three is not
                // auto-enabled by this option, despite the option's name.
                for index in 0..3 {
                    self.set_enabled(index, true, a);
                }
            }
            if !correct && self.options.auto_disable {
                self.set_enabled(slot, false, a);
            }
            for index in 0..4 {
                if index != slot
                    && let Some(other) = self.contestant(index)
                    && other.enabled
                {
                    Self::control(
                        a,
                        other.group,
                        6,
                        &[i64::from(correct)],
                        player.name.as_bytes().to_vec(),
                    );
                }
            }
            a.binary(Target::All, "BuzzerEOD_Answer", Vec::new());
            a.object(
                Target::Controller,
                if correct { 2 } else { 3 },
                vec![player.avatar_object],
            );
        }
        self.change_phase(DISABLED, a);
        self.roster(a);
    }
    fn index(bytes: &[u8]) -> Result<usize, Error> {
        let index = i32_payload(bytes)?;
        if !(0..4).contains(&index) {
            return Err(Error::InvalidMessage);
        }
        Ok(index as usize)
    }
    pub(super) fn message(&mut self, event: &str, payload: &[u8]) -> Result<Actions, Error> {
        if !self.joined || self.closing {
            return Err(Error::PluginNotReady);
        }
        let mut a = Actions::default();
        match event {
            "Buzzer_Host_A_ToggleMaster" => {
                let value = *payload.first().ok_or(Error::InvalidMessage)? > 0;
                self.change_phase(if value { READY } else { DISABLED }, &mut a);
                a.binary(
                    Target::All,
                    "Buzzer_Host_B_ToggleMaster",
                    vec![u8::from(value)],
                );
            }
            "Buzzer_Host_Request_Roster" => self.roster(&mut a),
            "Buzzer_Host_A_DeclareWinner" => {
                let index = Self::index(payload)?;
                if let Some(winner) = self.contestant(index).cloned() {
                    self.winner = Some((winner.group, winner.name.clone()));
                    for slot in 0..4 {
                        if slot != index
                            && let Some(player) = self.contestant(slot)
                        {
                            Self::control(
                                &mut a,
                                player.group,
                                9,
                                &[1],
                                winner.name.as_bytes().to_vec(),
                            );
                        }
                    }
                    a.text(Target::All, "Buzzer_Player_Win", winner.name);
                    a.object(Target::Controller, 4, vec![winner.avatar_object]);
                }
            }
            "Buzzer_Host_PlayerCorrect" | "Buzzer_Host_PlayerIncorrect" => self.judge(
                Self::index(payload)?,
                event == "Buzzer_Host_PlayerCorrect",
                &mut a,
            ),
            "Buzzer_Host_FindNewPlayer" => {
                let index = Self::index(payload)?;
                if self.phase == READY {
                    Self::no_changes(&mut a);
                } else {
                    self.fill(index, true, &mut a);
                }
            }
            "Buzzer_Host_MovePlayerLeft" | "Buzzer_Host_MovePlayerRight" => {
                let index = Self::index(payload)?;
                if self.phase == READY {
                    Self::no_changes(&mut a);
                } else {
                    let destination = if event == "Buzzer_Host_MovePlayerLeft" {
                        (index + 3) % 4
                    } else {
                        (index + 1) % 4
                    };
                    if self.slots[index].is_some() {
                        self.slots.swap(index, destination);
                    }
                    self.roster(&mut a);
                }
            }
            "Buzzer_Host_ToggleEnablePlayer" => {
                let index = Self::index(payload)?;
                if self.phase == READY {
                    Self::no_changes(&mut a);
                } else if let Some(player) = self.contestant(index) {
                    self.set_enabled(index, !player.enabled, &mut a);
                }
                self.roster(&mut a);
            }
            "Buzzer_Host_A_Deduct" | "Buzzer_Host_A_Disable" | "Buzzer_Host_A_Enable" => {
                let value = *payload.first().ok_or(Error::InvalidMessage)? != 0;
                let (field, success, failure) = match event {
                    "Buzzer_Host_A_Deduct" => (
                        &mut self.options.auto_deduct,
                        "Buzzer_Host_B_Deduct",
                        "Buzzer_Host_F_Deduct",
                    ),
                    "Buzzer_Host_A_Disable" => (
                        &mut self.options.auto_disable,
                        "Buzzer_Host_B_Disable",
                        "Buzzer_Host_F_Disable",
                    ),
                    _ => (
                        &mut self.options.auto_enable,
                        "Buzzer_Host_B_Enable",
                        "Buzzer_Host_F_Enable",
                    ),
                };
                let output = if self.phase == READY {
                    failure
                } else {
                    *field = value;
                    success
                };
                a.binary(Target::All, output, vec![u8::from(*field)]);
            }
            "Buzzer_Host_A_BuzzerTime"
            | "Buzzer_Host_A_AnswerTime"
            | "Buzzer_Host_A_GlobalScore" => {
                let (field, min, max, response, over, under) = match event {
                    "Buzzer_Host_A_BuzzerTime" => (
                        &mut self.options.buzzer_time,
                        2,
                        120,
                        "Buzzer_Host_B_BuzzerTime",
                        "Buzzer_Host_B_OverBuzzerTime",
                        "Buzzer_Host_B_UnderBuzzerTime",
                    ),
                    "Buzzer_Host_A_AnswerTime" => (
                        &mut self.options.answer_time,
                        2,
                        120,
                        "Buzzer_Host_B_AnswerTime",
                        "Buzzer_Host_B_OverAnswerTime",
                        "Buzzer_Host_B_UnderAnswerTime",
                    ),
                    _ => (
                        &mut self.options.score,
                        0,
                        9999,
                        "Buzzer_Host_B_GlobalScore",
                        "Buzzer_Host_B_OverGlobalScore",
                        "Buzzer_Host_B_UnderGlobalScore",
                    ),
                };
                if self.phase == READY {
                    Self::no_changes(&mut a);
                } else {
                    let value = short_payload(payload).unwrap_or(-1);
                    if value < min {
                        a.binary(Target::All, under, Vec::new());
                    } else if value > max {
                        a.binary(Target::All, over, Vec::new());
                    } else {
                        *field = value;
                    }
                }
                a.binary(Target::All, response, field.to_le_bytes().to_vec());
            }
            "Buzzer_Host_A_PlayerScore0"
            | "Buzzer_Host_A_PlayerScore1"
            | "Buzzer_Host_A_PlayerScore2"
            | "Buzzer_Host_A_PlayerScore3" => {
                let index = usize::from(event.as_bytes()[event.len() - 1] - b'0');
                if self.phase == READY {
                    Self::no_changes(&mut a);
                } else if let Some(player) = self.contestant_mut(index) {
                    let value = short_payload(payload).unwrap_or(-1);
                    if value < 0 {
                        a.binary(Target::All, "Buzzer_Host_B_UnderPlayerScore", Vec::new());
                    } else if value > 9999 {
                        a.binary(Target::All, "Buzzer_Host_B_OverPlayerScore", Vec::new());
                    } else {
                        player.score = value;
                        Self::control(&mut a, player.group, 4, &[i64::from(value), 1], Vec::new());
                    }
                }
                self.roster(&mut a);
            }
            _ => return Err(Error::EventNotAllowed),
        }
        Ok(a)
    }
    fn remove(&mut self, group: InstanceId, a: &mut Actions) {
        let answered = self
            .answering
            .and_then(|index| self.slots[usize::from(index)])
            == Some(group);
        for slot in &mut self.slots {
            if *slot == Some(group) {
                *slot = None;
            }
        }
        self.contestants.retain(|player| player.group != group);
        if self.winner.as_ref().is_some_and(|winner| winner.0 == group) {
            self.winner = None;
        }
        self.roster(a);
        if answered {
            self.broadcast(a, 10, 0, true, None);
            a.object(Target::Controller, 3, vec![0]);
            self.change_phase(DISABLED, a);
        }
    }
    pub(super) fn peer(
        &mut self,
        source: &PeerSource,
        signal: &Signal,
        roster: &Roster,
    ) -> Result<Actions, Error> {
        let mut a = Actions::default();
        if source.plugin != BUZZER_PLAYER || self.closing {
            return Ok(a);
        }
        if let Some(own) = self.own_group {
            if own != source.destination {
                return Err(Error::InvalidPluginInput);
            }
        } else {
            self.own_group = Some(source.destination);
        }
        if signal.code == REMOVE {
            self.remove(source.group, &mut a);
            return Ok(a);
        }
        if signal.code == SYNC {
            return Ok(a);
        }
        if !valid_single(&source.roster) {
            return Err(Error::InvalidPluginInput);
        }
        let Some(member) = first_member(&source.roster) else {
            return Ok(a);
        };
        if signal.code == ANNOUNCE {
            if signal.numbers.len() != 4 {
                return Err(Error::InvalidPluginInput);
            }
            if signal.numbers[0] != 0 && decode_id(signal.numbers[0]) != source.destination {
                return Ok(a);
            }
            let score = i16::try_from(signal.numbers[1]).map_err(|_| Error::InvalidPluginInput)?;
            let enabled = match signal.numbers[2] {
                0 => false,
                1 => true,
                _ => return Err(Error::InvalidPluginInput),
            };
            let owner = if signal.numbers[3] == 0 {
                None
            } else {
                Some(decode_id(signal.numbers[3]))
            };
            let name = parse_name(&signal.bytes)?;
            if let Some(player) = self
                .contestants
                .iter_mut()
                .find(|p| p.group == source.group)
            {
                if player.avatar_id != member.avatar_id
                    || player.avatar_object != member.avatar_object
                {
                    return Err(Error::InvalidPluginInput);
                }
                player.score = score;
                player.enabled = enabled;
                player.owner = owner;
                player.name = name;
            } else {
                if self.contestants.len() >= 16
                    || self.contestants.iter().any(|p| {
                        p.avatar_id == member.avatar_id || p.avatar_object == member.avatar_object
                    })
                {
                    return Err(Error::ParticipantLimit);
                }
                self.revision = self
                    .revision
                    .checked_add(1)
                    .filter(|v| *v != u64::MAX)
                    .ok_or(Error::CounterExhausted)?;
                self.contestants.push(Contestant {
                    group: source.group,
                    avatar_id: member.avatar_id,
                    avatar_object: member.avatar_object,
                    name,
                    score,
                    enabled,
                    owner,
                    visited: 0,
                });
            }
            if owner.is_some() && owner != self.own_group {
                for slot in &mut self.slots {
                    if *slot == Some(source.group) {
                        *slot = None;
                    }
                }
            }
            if self.joined && first_member(roster).is_some() {
                self.fill_empty(&mut a);
                self.roster(&mut a);
            }
        } else if signal.code == BUZZ {
            if signal.numbers.len() != 1
                || decode_id(signal.numbers[0]) != source.destination
                || !self.joined
            {
                return Ok(a);
            }
            let Some(index) = self
                .slots
                .iter()
                .position(|slot| *slot == Some(source.group))
            else {
                return Ok(a);
            };
            let Some(player) = self.contestant(index).cloned() else {
                return Ok(a);
            };
            if !player.enabled || player.owner != self.own_group {
                return Ok(a);
            }
            if self.phase == READY {
                self.change_phase(ENGAGED, &mut a);
                self.answering = Some(index as u8);
                Self::control(&mut a, source.group, 7, &[1], Vec::new());
                a.binary(
                    Target::All,
                    "BuzzerEOD_Buzzed",
                    (index as i32).to_le_bytes().to_vec(),
                );
                a.object(Target::Controller, 1, vec![member.avatar_object]);
            } else if self.phase == ENGAGED {
                Self::control(&mut a, source.group, 7, &[0], Vec::new());
            }
        }
        Ok(a)
    }
    pub(super) fn vm_event(&mut self, input: &VmInput) -> Result<Actions, Error> {
        if !self.joined || self.closing {
            return Err(Error::PluginNotReady);
        }
        let mut a = Actions::default();
        match input {
            VmInput::BuzzerHostJudgmentFinished => {
                a.binary(Target::All, "Buzzer_Host_Round_Restart", Vec::new())
            }
            VmInput::BuzzerHostDeclareWinner => {
                if let Some((winner, name)) = self.winner.take()
                    && self.slots.contains(&Some(winner))
                {
                    Self::control(&mut a, winner, 9, &[2], name.into_bytes());
                }
            }
            _ => return Err(Error::InvalidPluginInput),
        }
        Ok(a)
    }
    pub(super) fn leave(&mut self) -> Actions {
        self.joined = false;
        self.closing = true;
        self.winner = None;
        self.phase = DISABLED;
        self.answering = None;
        self.engaged_ticks = 0;
        self.tock = 0;
        let mut a = Actions::default();
        for group in self.slots.iter_mut().filter_map(Option::take) {
            Self::control(&mut a, group, 2, &[], Vec::new());
        }
        self.contestants.clear();
        a
    }
    pub(super) fn validate(&self, roster: &Roster) -> bool {
        valid_single(roster)
            && self.joined == first_member(roster).is_some()
            && self.phase <= EXPIRED
            && self.tock < 30
            && (0..=120).contains(&self.timer)
            && self.engaged_ticks <= 30
            && (self.phase != ENGAGED
                || self.engaged_ticks > 0 && self.answering.is_some() && self.timer > 0)
            && (self.phase != LOCKED
                || self.answering.is_some() && self.engaged_ticks == 0 && self.timer > 0)
            && (self.phase != READY || self.timer > 0 && self.answering.is_none())
            && self
                .answering
                .is_none_or(|index| index < 4 && self.slots[usize::from(index)].is_some())
            && self
                .winner
                .as_ref()
                .is_none_or(|(group, name)| self.slots.contains(&Some(*group)) && name.len() <= 128)
            && self.contestants.len() <= 16
            && self.revision != 0
            && self.revision != u64::MAX
            && self.contestants.iter().enumerate().all(|(i, p)| {
                p.group.0 != 0
                    && p.avatar_id != 0
                    && p.avatar_object > 0
                    && p.name.len() <= 128
                    && !p.name.contains('\0')
                    && p.visited <= self.revision
                    && !self.contestants[..i].iter().any(|q| {
                        p.group == q.group
                            || p.avatar_id == q.avatar_id
                            || p.avatar_object == q.avatar_object
                    })
            })
            && self.slots.iter().enumerate().all(|(i, slot)| {
                slot.is_none_or(|group| {
                    !self.slots[..i].contains(&Some(group))
                        && self
                            .contestants
                            .iter()
                            .any(|p| p.group == group && p.owner == self.own_group)
                })
            })
            && (0..=9999).contains(&self.options.score)
            && (2..=120).contains(&self.options.answer_time)
            && (2..=120).contains(&self.options.buzzer_time)
    }
    pub(super) fn validate_peers(
        &self,
        own_group: InstanceId,
        cluster: u64,
        peers: &[PeerGroup],
    ) -> bool {
        self.own_group.is_none_or(|group| group == own_group)
            && (self.own_group.is_some() || self.contestants.is_empty())
            && self.contestants.iter().all(|player| {
                linked_peer(peers, player.group, cluster, BUZZER_PLAYER).is_some_and(|peer| {
                    valid_single(&peer.roster)
                        && first_member(&peer.roster).is_some_and(|member| {
                            member.avatar_id == player.avatar_id
                                && member.avatar_object == player.avatar_object
                        })
                }) && player.owner.is_none_or(|owner| {
                    linked_peer(peers, owner, cluster, BUZZER_HOST).is_some_and(|peer| {
                        valid_single(&peer.roster) && first_member(&peer.roster).is_some()
                    })
                })
            })
    }
    pub(super) fn save(&self, w: &mut Writer) {
        save_option_id(self.own_group, w);
        w.bool(self.joined);
        w.bool(self.closing);
        w.u8(self.contestants.len() as u8);
        for p in &self.contestants {
            w.u64(p.group.0);
            w.u32(p.avatar_id);
            w.i16(p.avatar_object);
            w.string(&p.name);
            w.i16(p.score);
            w.bool(p.enabled);
            save_option_id(p.owner, w);
            w.u64(p.visited);
        }
        for slot in self.slots {
            save_option_id(slot, w);
        }
        w.u8(self.phase);
        w.u8(self.tock);
        w.i16(self.timer);
        w.u8(self.engaged_ticks);
        w.u8(self.answering.unwrap_or(255));
        w.bool(self.winner.is_some());
        if let Some((group, name)) = &self.winner {
            w.u64(group.0);
            w.string(name);
        }
        w.bool(self.options.auto_enable);
        w.bool(self.options.auto_disable);
        w.bool(self.options.auto_deduct);
        w.i16(self.options.score);
        w.i16(self.options.answer_time);
        w.i16(self.options.buzzer_time);
        w.u64(self.revision);
    }
    pub(super) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let own_group = restore_option_id(r)?;
        let joined = r.bool()?;
        let closing = r.bool()?;
        let count = r.u8()?;
        if count > 16 {
            return Err(Error::InvalidCheckpoint);
        }
        let mut contestants = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            contestants.push(Contestant {
                group: InstanceId(r.u64()?),
                avatar_id: r.u32()?,
                avatar_object: r.i16()?,
                name: r.string(128)?,
                score: r.i16()?,
                enabled: r.bool()?,
                owner: restore_option_id(r)?,
                visited: r.u64()?,
            });
        }
        let mut slots = [None; 4];
        for slot in &mut slots {
            *slot = restore_option_id(r)?;
        }
        let phase = r.u8()?;
        let tock = r.u8()?;
        let timer = r.i16()?;
        let engaged_ticks = r.u8()?;
        let answering = match r.u8()? {
            255 => None,
            value => Some(value),
        };
        let winner = if r.bool()? {
            Some((InstanceId(r.u64()?), r.string(128)?))
        } else {
            None
        };
        let options = Options {
            auto_enable: r.bool()?,
            auto_disable: r.bool()?,
            auto_deduct: r.bool()?,
            score: r.i16()?,
            answer_time: r.i16()?,
            buzzer_time: r.i16()?,
        };
        Ok(Self {
            own_group,
            joined,
            closing,
            contestants,
            slots,
            phase,
            tock,
            timer,
            engaged_ticks,
            answering,
            winner,
            options,
            revision: r.u64()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn member(id: u32, object: i16) -> Member {
        Member {
            seat: 0,
            actor: crate::ActorId(u64::from(id)),
            avatar_object: object,
            avatar_id: id,
            input: MemberInput::default(),
        }
    }
    #[test]
    fn checkpoint_peer_references_require_original_cluster_plugin_and_avatar() {
        let mut host = Host::new();
        host.joined = true;
        host.own_group = Some(InstanceId(10));
        host.contestants.push(Contestant {
            group: InstanceId(11),
            avatar_id: 1,
            avatar_object: 101,
            name: "Avatar 101".to_owned(),
            score: 30,
            enabled: false,
            owner: Some(InstanceId(10)),
            visited: 0,
        });
        host.slots[0] = Some(InstanceId(11));
        let mut host_roster = [None; 16];
        host_roster[0] = Some(member(5, 105));
        let mut player_roster = [None; 16];
        player_roster[0] = Some(member(1, 101));
        let peers = [
            PeerGroup {
                group: InstanceId(10),
                plugin: BUZZER_HOST,
                object: 500,
                cluster: 20,
                closing: false,
                roster: host_roster,
            },
            PeerGroup {
                group: InstanceId(11),
                plugin: BUZZER_PLAYER,
                object: 501,
                cluster: 20,
                closing: false,
                roster: player_roster,
            },
        ];
        let mut writer = Writer::default();
        host.save(&mut writer);
        let mut reader = Reader::new(&writer.0);
        let restored = Host::restore(&mut reader).unwrap();
        reader.finish().unwrap();
        assert!(restored.validate(&host_roster));
        assert!(restored.validate_peers(InstanceId(10), 20, &peers));
        assert!(!restored.validate_peers(InstanceId(12), 20, &peers));
        assert!(!restored.validate_peers(InstanceId(10), 21, &peers));
        assert!(!restored.validate_peers(InstanceId(10), 20, &peers[..1]));
        let mut forged = peers;
        forged[1].plugin = DJ_STATION;
        assert!(!restored.validate_peers(InstanceId(10), 20, &forged));
        forged = peers;
        forged[1].closing = true;
        assert!(!restored.validate_peers(InstanceId(10), 20, &forged));
        forged = peers;
        forged[1].roster[0].as_mut().unwrap().avatar_id = 2;
        assert!(!restored.validate_peers(InstanceId(10), 20, &forged));
        forged = peers;
        forged[1].roster[0].as_mut().unwrap().avatar_object = 102;
        assert!(!restored.validate_peers(InstanceId(10), 20, &forged));
        forged = peers;
        forged[0].roster = [None; 16];
        assert!(!restored.validate_peers(InstanceId(10), 20, &forged));
    }
}
