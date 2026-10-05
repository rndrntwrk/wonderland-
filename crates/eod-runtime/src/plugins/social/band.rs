// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
use super::{util::*, *};

const LOBBY: u8 = 0;
const PRESHOW: u8 = 1;
const REHEARSAL: u8 = 2;
const PERFORMANCE: u8 = 3;
const INTERMISSION: u8 = 4;
const FINALE: u8 = 5;
const MIN_PAYMENT: u8 = 7;
const ELECTRIC: u8 = 8;

#[derive(Clone)]
pub(super) struct Band {
    names: Vec<AvatarName>,
    buzz_rng: Rng,
    note_rng: Rng,
    phase: u8,
    next: Option<u8>,
    song: [u8; 25],
    length: u8,
    note: u8,
    note_open: bool,
    timer: i16,
    frame: u8,
    sequence_ticks: u16,
    combined_hundredths: i32,
    decisions: [Option<bool>; 4],
}
impl Band {
    pub(super) fn new(seed: Seed, names: Vec<AvatarName>) -> Result<Self, Error> {
        if !validate_names(&names) {
            return Err(Error::InvalidPluginInput);
        }
        Ok(Self {
            names,
            buzz_rng: Rng::new(seed.0),
            note_rng: Rng::new(seed.0 ^ 0x46fd155ad301dec1),
            phase: LOBBY,
            next: None,
            song: [0; 25],
            length: 0,
            note: 0,
            note_open: false,
            timer: -1,
            frame: 0,
            sequence_ticks: 0,
            combined_hundredths: 0,
            decisions: [None; 4],
        })
    }
    fn players(roster: &Roster) -> String {
        (0..4)
            .map(|role| {
                by_role(roster, role)
                    .map_or(0, |m| m.avatar_object)
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    fn enqueue(&mut self, phase: u8) {
        if self.phase != phase {
            self.next = Some(phase);
        }
    }
    fn set_timer(&mut self, timer: i16, a: &mut Actions) {
        self.timer = timer;
        self.frame = 0;
        a.text(Target::All, "Band_Timer", timer.max(0).to_string());
    }
    pub(super) fn join(&mut self, member: Member, roster: &Roster) -> Result<Actions, Error> {
        if !self.valid_roster(roster) {
            return Err(Error::InvalidPluginInput);
        }
        name_for(&self.names, member)?;
        let mut a = Actions::default();
        a.text(Target::All, "Band_Players", Self::players(roster));
        a.binary(
            Target::Member(member.seat),
            "Band_UI_Init",
            vec![member.input.role],
        );
        if full(roster, 4) {
            self.enqueue(PRESHOW);
        }
        Ok(a)
    }
    fn payout(length: u8) -> i16 {
        let mut cumulative = 0i16;
        for index in 1..=i16::from(length) {
            if index == 1 {
                cumulative = 40;
            } else {
                cumulative += index * index + (20 - index) * 5;
                if index % 5 == 0 {
                    cumulative += 200;
                }
            }
        }
        cumulative
    }
    fn skill_payout(&self) -> i16 {
        // Decimal Math.Round uses midpoint-to-even. Preserve this with exact
        // integer hundredths instead of a binary floating point approximation.
        let numerator = self.combined_hundredths * 18 * i32::from(self.length);
        let whole = numerator / 2500;
        let remainder = numerator % 2500;
        (whole + i32::from(remainder > 1250 || remainder == 1250 && whole % 2 != 0)) as i16
    }
    fn skill_display(&self) -> String {
        if self.combined_hundredths % 100 == 0 {
            (self.combined_hundredths / 100).to_string()
        } else {
            format!(
                "{}.{:02}",
                self.combined_hundredths / 100,
                self.combined_hundredths % 100
            )
            .trim_end_matches('0')
            .to_owned()
        }
    }
    fn win(&self, a: &mut Actions) {
        let payout = Self::payout(self.length).to_string();
        let length = self.length.to_string();
        a.binary(
            Target::All,
            "Band_Win",
            strings([payout.as_str(), length.as_str()]),
        );
        a.object(Target::Controller, 5, vec![i16::from(self.length)]);
        a.object(Target::Controller, 6, vec![self.skill_payout()]);
        a.object(Target::Controller, 3, vec![Self::payout(self.length)]);
    }
    fn game_over(
        &mut self,
        win: bool,
        event: Option<&'static str>,
        a: &mut Actions,
    ) -> Result<(), Error> {
        self.set_timer(-1, a);
        self.note_open = false;
        self.sequence_ticks = 0;
        self.enqueue(FINALE);
        if win {
            self.win(a);
        } else {
            self.length = self
                .length
                .checked_sub(1)
                .ok_or(Error::InvalidPluginInput)?;
            if let Some(event) = event {
                a.binary(Target::All, event, Vec::new());
            }
            if self.length >= 5 {
                self.enqueue(MIN_PAYMENT);
            }
            a.object(Target::Controller, 2, Vec::new());
        }
        Ok(())
    }
    fn change_phase(&mut self, phase: u8, roster: &Roster, a: &mut Actions) -> Result<(), Error> {
        if phase != LOBBY && !full(roster, 4) {
            self.enqueue(LOBBY);
            return Ok(());
        }
        self.phase = phase;
        match phase {
            LOBBY => {
                self.set_timer(-1, a);
                self.note_open = false;
                self.sequence_ticks = 0;
            }
            PRESHOW => {
                self.set_timer(-1, a);
                self.length = 0;
                self.note = 0;
                self.note_open = false;
                self.sequence_ticks = 0;
                for note in &mut self.song {
                    *note = if self.buzz_rng.below(32766)? + 1 > 72 {
                        (self.note_rng.below(8)? + 1) as u8
                    } else {
                        0
                    };
                }
                self.combined_hundredths = roster
                    .iter()
                    .flatten()
                    .map(|m| {
                        i32::from(
                            m.input.skills[match m.input.role {
                                0 => 0,
                                1 => 1,
                                _ => 2,
                            }],
                        )
                    })
                    .sum();
                let decimal = self.skill_display();
                a.binary(
                    Target::All,
                    "Band_Game_Reset_Skill",
                    strings([decimal.as_str()]),
                );
                self.set_timer(10, a);
                a.binary(Target::All, "Band_Show", Vec::new());
            }
            REHEARSAL => {
                self.set_timer(-1, a);
                self.decisions = [None; 4];
                self.note = 0;
                self.note_open = false;
                self.length = self
                    .length
                    .checked_add(1)
                    .filter(|length| *length <= 25)
                    .ok_or(Error::InvalidPluginInput)?;
                self.sequence_ticks = 45 * (u16::from(self.length) + 2);
                a.binary(
                    Target::All,
                    "Band_Sequence",
                    self.song[..usize::from(self.length)].to_vec(),
                );
            }
            PERFORMANCE => {
                self.note_open = true;
                a.binary(Target::All, "Band_Performance", Vec::new());
                self.set_timer(10, a);
            }
            INTERMISSION => {
                let payout = Self::payout(self.length).to_string();
                let next = if self.length < 25 {
                    Self::payout(self.length + 1).to_string()
                } else {
                    String::new()
                };
                let length = self.length.to_string();
                a.binary(
                    Target::All,
                    "Band_Intermission",
                    strings([payout.as_str(), next.as_str(), length.as_str()]),
                );
                self.set_timer(10, a);
            }
            MIN_PAYMENT => {
                self.length -= self.length % 5;
            }
            ELECTRIC => {
                self.set_timer(-1, a);
                a.binary(Target::All, "Band_Electric", Vec::new());
            }
            FINALE => {}
            _ => return Err(Error::InvalidPluginInput),
        }
        Ok(())
    }
    fn rock_on(&mut self, expired: bool, roster: &Roster, a: &mut Actions) -> bool {
        let mut yes = 0;
        let mut no = 0;
        for role in 0..4 {
            match self.decisions[usize::from(role)] {
                Some(true) => yes += 1,
                Some(false) => no += 1,
                None if expired => {
                    self.decisions[usize::from(role)] = Some(true);
                    yes += 1;
                    role_text(a, roster, role, "Band_RockOn", String::new());
                }
                None => {}
            }
        }
        if no > 2 {
            self.set_timer(0, a);
        }
        yes > 1
    }
    pub(super) fn tick(&mut self, roster: &Roster) -> Result<Actions, Error> {
        let mut a = Actions::default();
        if let Some(phase) = self.next.take() {
            self.change_phase(phase, roster, &mut a)?;
        }
        if self.timer > 0 {
            self.frame += 1;
            if self.frame >= 30 {
                self.frame = 0;
                self.timer -= 1;
                a.text(Target::All, "Band_Timer", self.timer.to_string());
            }
        }
        match self.phase {
            PRESHOW if self.timer == 0 => self.enqueue(REHEARSAL),
            PERFORMANCE if self.timer == 0 => {
                self.game_over(false, Some("Band_Timeout"), &mut a)?
            }
            INTERMISSION => {
                if self.rock_on(false, roster, &mut a) {
                    self.enqueue(REHEARSAL);
                } else if self.timer == 0 {
                    if self.rock_on(true, roster, &mut a) {
                        self.enqueue(REHEARSAL);
                    } else {
                        self.game_over(true, None, &mut a)?;
                    }
                }
            }
            FINALE if self.timer == 0 => self.enqueue(LOBBY),
            _ => {}
        }
        if self.sequence_ticks > 0 {
            self.sequence_ticks -= 1;
            if self.sequence_ticks == 0 {
                self.enqueue(PERFORMANCE);
            }
        }
        Ok(a)
    }
    pub(super) fn message(
        &mut self,
        member: Member,
        event: &str,
        payload: &[u8],
        roster: &Roster,
    ) -> Result<Actions, Error> {
        let mut a = Actions::default();
        if event == "Band_Decision" {
            // The source records a decision whenever a valid participant sends
            // one, and clears it when the next rehearsal starts.
            self.decisions[usize::from(member.input.role)] =
                Some(*payload.first().ok_or(Error::InvalidMessage)? == 1);
            return Ok(a);
        }
        if event != "Band_Note" {
            return Err(Error::EventNotAllowed);
        }
        let note = *payload.first().ok_or(Error::InvalidMessage)?;
        if self.phase != PERFORMANCE || !self.note_open || !full(roster, 4) {
            return Err(Error::PluginNotReady);
        }
        if note != 0 && (note > 8 || (note - 1) / 2 != member.input.role) {
            return Ok(a);
        }
        self.note_open = false;
        a.binary(Target::All, "Band_Note_Sync", vec![note]);
        if self.song[usize::from(self.note)] == note {
            if note == 0 {
                self.game_over(false, Some("Band_Buzz"), &mut a)?;
            } else if self.note == self.length - 1 {
                self.set_timer(-1, &mut a);
                if self.length == 25 {
                    self.game_over(true, None, &mut a)?;
                } else {
                    a.object(Target::Controller, 7, vec![i16::from(member.input.role)]);
                    self.enqueue(ELECTRIC);
                }
            } else {
                self.note += 1;
                self.note_open = true;
                a.binary(Target::All, "Band_Continue_Performance", Vec::new());
                self.set_timer(10, &mut a);
            }
        } else {
            let name = name_for(&self.names, member)?;
            if !name.is_empty() {
                a.text(Target::All, "Band_Fail", name.to_owned());
            }
            self.game_over(false, None, &mut a)?;
        }
        Ok(a)
    }
    pub(super) fn vm_event(&mut self, input: &VmInput, roster: &Roster) -> Result<Actions, Error> {
        if !matches!(input, VmInput::BandAnimationsFinished) {
            return Err(Error::InvalidPluginInput);
        }
        let mut a = Actions::default();
        if self.phase == MIN_PAYMENT {
            // Finale admission and the three payout requests are one host
            // transaction. A duplicate callback before that queued phase runs
            // must not issue the same payment request a second time.
            if self.next.is_some() {
                return Err(Error::PluginNotReady);
            }
            self.enqueue(FINALE);
            self.win(&mut a);
        } else if full(roster, 4) {
            self.enqueue(if self.phase == ELECTRIC {
                INTERMISSION
            } else {
                PRESHOW
            });
        }
        Ok(a)
    }
    pub(super) fn leave(&mut self, roster: &Roster) -> Actions {
        self.enqueue(LOBBY);
        self.sequence_ticks = 0;
        self.note_open = false;
        let mut a = Actions::default();
        a.text(Target::All, "Band_Players", Self::players(roster));
        a
    }
    pub(super) fn rebind(&self, member: Member, roster: &Roster) -> Actions {
        let mut a = Actions::default();
        let target = Target::Member(member.seat);
        a.text(target, "Band_Players", Self::players(roster));
        a.binary(target, "Band_UI_Init", vec![member.input.role]);
        if self.phase != LOBBY {
            // Original UI_Init enters the lobby and hides every game button.
            // Restore the saved display before replaying phase-specific input.
            let skill = self.skill_display();
            a.binary(target, "Band_Game_Reset_Skill", strings([skill.as_str()]));
            a.binary(target, "Band_Show", Vec::new());
        }
        a.text(target, "Band_Timer", self.timer.max(0).to_string());
        match self.phase {
            REHEARSAL => a.binary(
                target,
                "Band_Sequence",
                self.song[..usize::from(self.length)].to_vec(),
            ),
            PERFORMANCE if self.note_open => a.binary(target, "Band_Performance", Vec::new()),
            ELECTRIC => a.binary(target, "Band_Electric", Vec::new()),
            INTERMISSION => {
                let payout = Self::payout(self.length).to_string();
                let next = if self.length < 25 {
                    Self::payout(self.length + 1).to_string()
                } else {
                    String::new()
                };
                let length = self.length.to_string();
                a.binary(
                    target,
                    "Band_Intermission",
                    strings([payout.as_str(), next.as_str(), length.as_str()]),
                );
            }
            _ => {}
        }
        a
    }
    fn valid_roster(&self, roster: &Roster) -> bool {
        valid_roles(roster, 4)
            && roster.iter().flatten().all(|m| {
                name_for(&self.names, *m).is_ok()
                    && m.input.skills[..3]
                        .iter()
                        .all(|skill| (0..=1000).contains(skill))
            })
    }
    pub(super) fn validate(&self, roster: &Roster) -> bool {
        let valid_phase = |p| {
            matches!(
                p,
                LOBBY
                    | PRESHOW
                    | REHEARSAL
                    | PERFORMANCE
                    | INTERMISSION
                    | FINALE
                    | MIN_PAYMENT
                    | ELECTRIC
            )
        };
        self.valid_roster(roster)
            && validate_names(&self.names)
            && valid_phase(self.phase)
            && self.next.is_none_or(valid_phase)
            && self.length <= 25
            && self.note <= 24
            && self.song.iter().all(|note| *note <= 8)
            && (-1..=10).contains(&self.timer)
            && self.frame < 30
            && (0..=4000).contains(&self.combined_hundredths)
            && self.sequence_ticks <= 1215
            && (!self.note_open
                || self.phase == PERFORMANCE && self.note < self.length && self.next.is_none())
            && (self.sequence_ticks == 0 || self.phase == REHEARSAL && self.length > 0)
            && (self.phase == LOBBY || full(roster, 4) || self.next == Some(LOBBY))
            && (!matches!(
                self.phase,
                REHEARSAL | PERFORMANCE | INTERMISSION | ELECTRIC
            ) || self.length > 0
                || self.next.is_some())
    }
    pub(super) fn save(&self, w: &mut Writer) {
        save_names(&self.names, w);
        self.buzz_rng.save(w);
        self.note_rng.save(w);
        w.u8(self.phase);
        w.u8(self.next.unwrap_or(255));
        w.fixed(&self.song);
        w.u8(self.length);
        w.u8(self.note);
        w.bool(self.note_open);
        w.i16(self.timer);
        w.u8(self.frame);
        w.u16(self.sequence_ticks);
        w.i32(self.combined_hundredths);
        for decision in self.decisions {
            w.u8(match decision {
                None => 0,
                Some(false) => 1,
                Some(true) => 2,
            });
        }
    }
    pub(super) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let names = restore_names(r)?;
        let buzz_rng = Rng::restore(r)?;
        let note_rng = Rng::restore(r)?;
        let phase = r.u8()?;
        let next = match r.u8()? {
            255 => None,
            v => Some(v),
        };
        let song = r
            .take(25)?
            .try_into()
            .map_err(|_| Error::InvalidCheckpoint)?;
        let length = r.u8()?;
        let note = r.u8()?;
        let note_open = r.bool()?;
        let timer = r.i16()?;
        let frame = r.u8()?;
        let sequence_ticks = r.u16()?;
        let combined_hundredths = r.i32()?;
        let mut decisions = [None; 4];
        for decision in &mut decisions {
            *decision = match r.u8()? {
                0 => None,
                1 => Some(false),
                2 => Some(true),
                _ => return Err(Error::InvalidCheckpoint),
            };
        }
        Ok(Self {
            names,
            buzz_rng,
            note_rng,
            phase,
            next,
            song,
            length,
            note,
            note_open,
            timer,
            frame,
            sequence_ticks,
            combined_hundredths,
            decisions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn payout_milestones_and_decimal_midpoint_rounding() {
        assert_eq!(
            [0, 1, 5, 10, 15, 20, 25].map(Band::payout),
            [0, 40, 624, 1454, 2684, 4564, 7344]
        );
        let mut band = Band::new(Seed::new(1), Vec::new()).unwrap();
        band.length = 25;
        band.combined_hundredths = 25;
        assert_eq!(band.skill_payout(), 4);
        band.combined_hundredths = 75;
        assert_eq!(band.skill_payout(), 14);
    }
}
