// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
use super::{util::*, *};

pub(super) const SCAN: u16 = 100;
pub(super) const ANNOUNCE: u16 = 101;
pub(super) const BUZZ: u16 = 102;
pub(super) const SYNC: u16 = 103;
pub(super) const CONTROL: u16 = 104;
pub(super) const REMOVE: u16 = 105;

#[derive(Clone)]
pub(super) struct Player {
    names: Vec<AvatarName>,
    score: i16,
    enabled: bool,
    master: bool,
    timer: i16,
    last_buzz: Option<bool>,
    host: Option<InstanceId>,
    joined: bool,
    queued: Vec<(i16, i16)>,
}
impl Player {
    pub(super) fn required_peers(&self) -> Vec<InstanceId> {
        self.host.into_iter().collect()
    }

    pub(super) fn new(names: Vec<AvatarName>) -> Result<Self, Error> {
        if !validate_names(&names) {
            return Err(Error::InvalidPluginInput);
        }
        Ok(Self {
            names,
            score: 0,
            enabled: false,
            master: false,
            timer: 0,
            last_buzz: None,
            host: None,
            joined: false,
            queued: Vec::new(),
        })
    }
    fn announce(&self, roster: &Roster, target: Option<InstanceId>) -> Result<Actions, Error> {
        let mut a = Actions::default();
        if let Some(member) = first_member(roster) {
            let name = name_for(&self.names, member)?;
            a.peer(
                BUZZER_HOST,
                signal(
                    ANNOUNCE,
                    vec![
                        target.map_or(0, target_id),
                        i64::from(self.score),
                        i64::from(self.enabled),
                        self.host.map_or(0, target_id),
                    ],
                    name.as_bytes().to_vec(),
                ),
            );
        }
        Ok(a)
    }
    pub(super) fn join(&mut self, member: Member, roster: &Roster) -> Result<Actions, Error> {
        if !valid_single(roster) || self.joined {
            return Err(Error::InvalidPluginInput);
        }
        name_for(&self.names, member)?;
        self.score = member.input.registers[0];
        self.enabled = false;
        self.master = false;
        self.timer = 0;
        self.last_buzz = None;
        self.host = None;
        self.joined = true;
        self.queued.clear();
        let mut a = Actions::default();
        a.binary(Target::Member(member.seat), "BuzzerEOD_Init", vec![2]);
        a.binary(
            Target::Member(member.seat),
            "Buzzer_Player_Score",
            self.score.to_le_bytes().to_vec(),
        );
        append(&mut a, self.announce(roster, None)?);
        Ok(a)
    }
    pub(super) fn rebind(&self, member: Member) -> Actions {
        let mut a = Actions::default();
        a.binary(Target::Member(member.seat), "BuzzerEOD_Init", vec![2]);
        a.binary(
            Target::Member(member.seat),
            "Buzzer_Player_Score",
            self.score.to_le_bytes().to_vec(),
        );
        a.binary(
            Target::Member(member.seat),
            "BuzzerEOD_Master",
            vec![u8::from(self.master)],
        );
        a.text(
            Target::Member(member.seat),
            "BuzzerEOD_Timer",
            self.timer.to_string(),
        );
        if let Some(first) = self.last_buzz {
            a.binary(
                Target::Member(member.seat),
                "BuzzerEOD_Buzzed",
                i16::from(first).to_le_bytes().to_vec(),
            );
        }
        a
    }
    pub(super) fn message(&mut self, event: &str) -> Result<Actions, Error> {
        if event != "Buzzer_Player_Buzzed" {
            return Err(Error::EventNotAllowed);
        }
        let mut a = Actions::default();
        if self.enabled
            && let Some(host) = self.host
        {
            a.peer(BUZZER_HOST, signal(BUZZ, vec![target_id(host)], Vec::new()));
        }
        Ok(a)
    }
    fn queue(&mut self, code: i16, argument: i16) -> Result<(), Error> {
        if self.queued.len() >= 32 {
            return Err(Error::QueueFull);
        }
        self.queued.push((code, argument));
        Ok(())
    }
    fn sync(&mut self) -> Actions {
        let mut a = Actions::default();
        // The player-specific score subscription precedes this player's base
        // queue subscription in the original event invocation list.
        a.object(Target::Controller, 5, vec![self.score]);
        for (code, argument) in self.queued.drain(..) {
            a.object(Target::Controller, code, vec![argument]);
        }
        a
    }
    pub(super) fn vm_event(&mut self, input: &VmInput) -> Result<Actions, Error> {
        if !matches!(input, VmInput::BuzzerPlayerSync) {
            return Err(Error::InvalidPluginInput);
        }
        if !self.joined {
            return Err(Error::PluginNotReady);
        }
        let mut a = self.sync();
        a.peer(BUZZER_PLAYER, signal(SYNC, Vec::new(), Vec::new()));
        a.peer(BUZZER_HOST, signal(SYNC, Vec::new(), Vec::new()));
        Ok(a)
    }
    pub(super) fn peer(
        &mut self,
        source: &PeerSource,
        signal: &Signal,
        roster: &Roster,
    ) -> Result<Actions, Error> {
        if source.plugin == BUZZER_PLAYER && signal.code == SYNC {
            if source.group != source.destination
                && self.joined
                && valid_single(&source.roster)
                && first_member(&source.roster).is_some()
            {
                return Ok(self.sync());
            }
            return Ok(Actions::default());
        }
        if source.plugin != BUZZER_HOST {
            return Ok(Actions::default());
        }
        if signal.code == SCAN {
            return self.announce(roster, Some(source.group));
        }
        if signal.code != CONTROL
            || signal.numbers.len() < 2
            || decode_id(signal.numbers[0]) != source.destination
        {
            return Ok(Actions::default());
        }
        let command = signal.numbers[1];
        if command == 1 {
            if valid_single(&source.roster)
                && first_member(&source.roster).is_some()
                && self.joined
                && (self.host.is_none() || self.host == Some(source.group))
            {
                self.host = Some(source.group);
            }
            return self.announce(roster, Some(source.group));
        }
        if self.host != Some(source.group) {
            return Ok(Actions::default());
        }
        if command == 2 {
            self.host = None;
            self.master = false;
            self.timer = 0;
            self.last_buzz = None;
            let mut a = self.announce(roster, None)?;
            a.binary(Target::All, "BuzzerEOD_Master", vec![0]);
            return Ok(a);
        }
        if !valid_single(&source.roster) || first_member(&source.roster).is_none() {
            return Err(Error::InvalidPluginInput);
        }
        let target = Target::All;
        let mut a = Actions::default();
        let number = |index: usize| {
            signal
                .numbers
                .get(index)
                .copied()
                .ok_or(Error::InvalidPluginInput)
        };
        match command {
            3 => {
                self.enabled = match number(2)? {
                    0 => false,
                    1 => true,
                    _ => return Err(Error::InvalidPluginInput),
                };
            }
            4 => {
                let score = i16::try_from(number(2)?).map_err(|_| Error::InvalidPluginInput)?;
                if !(0..=9999).contains(&score) {
                    return Err(Error::InvalidPluginInput);
                }
                self.score = score;
                if number(3)? == 1 {
                    a.object(Target::Controller, 5, vec![score]);
                }
                a.binary(target, "Buzzer_Player_Score", score.to_le_bytes().to_vec());
            }
            5 => {
                let correct = match number(2)? {
                    0 => false,
                    1 => true,
                    _ => return Err(Error::InvalidPluginInput),
                };
                let points = i16::try_from(number(3)?).map_err(|_| Error::InvalidPluginInput)?;
                if !(0..=9999).contains(&points) {
                    return Err(Error::InvalidPluginInput);
                }
                self.score = if correct {
                    i32::from(self.score)
                        .saturating_add(i32::from(points))
                        .min(9999) as i16
                } else {
                    i32::from(self.score)
                        .saturating_sub(i32::from(points))
                        .max(0) as i16
                };
                a.binary(
                    target,
                    "Buzzer_Player_Score",
                    self.score.to_le_bytes().to_vec(),
                );
                a.object(Target::Controller, 2, vec![i16::from(correct)]);
                a.binary(target, "BuzzerEOD_Answer", vec![u8::from(correct)]);
            }
            6 => {
                let correct = i16::try_from(number(2)?).map_err(|_| Error::InvalidPluginInput)?;
                if ![0, 1].contains(&correct) {
                    return Err(Error::InvalidPluginInput);
                }
                let name = parse_name(&signal.bytes)?;
                a.text(
                    target,
                    if correct == 0 {
                        "Buzzer_Player_Other_Incorrect"
                    } else {
                        "Buzzer_Player_Other_Correct"
                    },
                    name,
                );
                self.queue(4, correct)?;
            }
            7 => {
                let first = i16::try_from(number(2)?).map_err(|_| Error::InvalidPluginInput)?;
                if ![0, 1].contains(&first) {
                    return Err(Error::InvalidPluginInput);
                }
                self.last_buzz = Some(first == 1);
                a.object(Target::Controller, 1, vec![first]);
                a.binary(target, "BuzzerEOD_Buzzed", first.to_le_bytes().to_vec());
            }
            8 => {
                self.last_buzz = Some(false);
                a.object(Target::Controller, 3, Vec::new());
                a.binary(target, "BuzzerEOD_Buzzed", vec![0]);
            }
            9 => {
                let result = i16::try_from(number(2)?).map_err(|_| Error::InvalidPluginInput)?;
                if ![1, 2].contains(&result) {
                    return Err(Error::InvalidPluginInput);
                }
                a.text(target, "Buzzer_Player_Win", parse_name(&signal.bytes)?);
                if result == 2 {
                    a.object(Target::Controller, 6, vec![result]);
                } else {
                    self.queue(6, result)?;
                }
            }
            10 => {
                a.text(target, "Buzzer_Player_Other_Incorrect", String::new());
                a.object(Target::Controller, 4, vec![0]);
            }
            11 => {
                let enabled = match number(2)? {
                    0 => false,
                    1 => true,
                    _ => return Err(Error::InvalidPluginInput),
                };
                self.master = enabled;
                self.last_buzz = None;
                a.binary(target, "BuzzerEOD_Master", vec![u8::from(enabled)]);
            }
            12 => {
                let timer = number(2)?;
                if !(0..=120).contains(&timer) {
                    return Err(Error::InvalidPluginInput);
                }
                self.timer = timer as i16;
                a.text(target, "BuzzerEOD_Timer", timer.to_string());
            }
            _ => return Err(Error::InvalidPluginInput),
        }
        if matches!(command, 3..=5) {
            append(&mut a, self.announce(roster, Some(source.group))?);
        }
        Ok(a)
    }
    pub(super) fn leave(&mut self) -> Actions {
        self.joined = false;
        self.host = None;
        self.enabled = false;
        self.master = false;
        self.timer = 0;
        self.last_buzz = None;
        self.queued.clear();
        let mut a = Actions::default();
        a.peer(BUZZER_HOST, signal(REMOVE, Vec::new(), Vec::new()));
        a
    }
    pub(super) fn validate(&self, roster: &Roster) -> bool {
        validate_names(&self.names)
            && valid_single(roster)
            && self.joined == first_member(roster).is_some()
            && roster
                .iter()
                .flatten()
                .all(|member| name_for(&self.names, *member).is_ok())
            && self.queued.len() <= 32
            && (0..=120).contains(&self.timer)
            && self.queued.iter().all(|(code, arg)| {
                (*code == 4 && [0, 1].contains(arg)) || (*code == 6 && *arg == 1)
            })
            && (self.joined
                || self.host.is_none()
                    && !self.enabled
                    && !self.master
                    && self.timer == 0
                    && self.last_buzz.is_none()
                    && self.queued.is_empty())
    }
    pub(super) fn validate_peers(&self, cluster: u64, peers: &[PeerGroup]) -> bool {
        self.host.is_none_or(|group| {
            linked_peer(peers, group, cluster, BUZZER_HOST).is_some_and(|peer| {
                valid_single(&peer.roster) && first_member(&peer.roster).is_some()
            })
        })
    }
    pub(super) fn save(&self, w: &mut Writer) {
        save_names(&self.names, w);
        w.i16(self.score);
        w.bool(self.enabled);
        w.bool(self.master);
        w.i16(self.timer);
        w.u8(match self.last_buzz {
            None => 0,
            Some(false) => 1,
            Some(true) => 2,
        });
        save_option_id(self.host, w);
        w.bool(self.joined);
        w.u8(self.queued.len() as u8);
        for (code, arg) in &self.queued {
            w.i16(*code);
            w.i16(*arg);
        }
    }
    pub(super) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let names = restore_names(r)?;
        let score = r.i16()?;
        let enabled = r.bool()?;
        let master = r.bool()?;
        let timer = r.i16()?;
        let last_buzz = match r.u8()? {
            0 => None,
            1 => Some(false),
            2 => Some(true),
            _ => return Err(Error::InvalidCheckpoint),
        };
        let host = restore_option_id(r)?;
        let joined = r.bool()?;
        let count = r.u8()?;
        if count > 32 {
            return Err(Error::InvalidCheckpoint);
        }
        let mut queued = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            queued.push((r.i16()?, r.i16()?));
        }
        Ok(Self {
            names,
            score,
            enabled,
            master,
            timer,
            last_buzz,
            host,
            joined,
            queued,
        })
    }
}
pub(super) fn parse_name(bytes: &[u8]) -> Result<String, Error> {
    if bytes.len() > 128 {
        return Err(Error::InvalidPluginInput);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| Error::InvalidPluginInput)?;
    if text.contains('\0') {
        return Err(Error::InvalidPluginInput);
    }
    Ok(text.to_owned())
}
