// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! VMEODPaperChasePlugin and EODLobby source translation.
use super::{rng::NativeRng, *};

#[derive(Clone)]
pub(crate) struct PaperChase {
    phase: u8,
    ticks: u16,
    matches: i16,
    previous_matches: i16,
    combination: Option<[u8; 3]>,
    letters: [Option<u8>; 3],
    previous: [Option<u8>; 3],
    displayed: [i16; 7],
    rng: NativeRng,
}
impl PaperChase {
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            phase: 1,
            ticks: 0,
            matches: -1,
            previous_matches: -1,
            combination: None,
            letters: [None; 3],
            previous: [None; 3],
            displayed: [-1; 7],
            rng: NativeRng::new(seed),
        }
    }
    fn reset(&mut self) {
        self.matches = -1;
        self.previous_matches = -1;
        self.combination = None;
        self.letters = [None; 3];
        self.previous = [None; 3];
        self.ticks = 0;
    }
    fn letter_text(values: &[i16; 7]) -> String {
        values
            .iter()
            .map(i16::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    }
    fn broadcast_letters(&mut self, roster: &Roster, actions: &mut Actions) {
        for i in 0..3 {
            self.displayed[i] = self.letters[i].map_or(-1, i16::from);
            self.displayed[i + 3] = self.previous[i].map_or(-1, i16::from);
        }
        self.displayed[6] = self.previous_matches;
        actions.broadcast_text(
            roster,
            "paperchase_letters",
            Self::letter_text(&self.displayed),
        );
    }
    fn roster(roster: &Roster) -> String {
        roster[..3]
            .iter()
            .map(i16::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    }
    fn transition(
        &mut self,
        phase: u8,
        roster: &Roster,
        actions: &mut Actions,
    ) -> Result<(), Error> {
        if self.phase == phase {
            return Ok(());
        }
        self.phase = phase;
        actions.broadcast_text(roster, "paperchase_state", phase.to_string());
        match phase {
            1 => {
                self.reset();
                self.broadcast_letters(roster, actions);
            }
            2 => {
                self.reset();
                let index = self.rng.below(27)? as u8;
                self.combination = Some([index / 9 + 1, (index / 3) % 3 + 1, index % 3 + 1]);
                self.transition(3, roster, actions)?;
            }
            3 => {
                self.previous_matches = self.matches;
                self.broadcast_letters(roster, actions);
                actions.object(GameObjectEvent::PaperChaseIdle);
            }
            4 => {
                let combination = self.combination.ok_or(Error::InvalidPluginInput)?;
                self.matches = 0;
                for (seat, wanted) in combination.iter().enumerate() {
                    let letter = self.letters[seat].ok_or(Error::InvalidPluginInput)?;
                    if letter == *wanted {
                        self.matches += 1;
                    }
                    actions.object(GameObjectEvent::PaperChaseSetLetter(
                        i16::from(letter) | ((seat as i16 + 1) << 8),
                    ));
                    self.previous[seat] = Some(letter);
                    self.letters[seat] = None;
                }
                actions.object(GameObjectEvent::PaperChaseSetResult(self.matches));
                self.transition(5, roster, actions)?;
            }
            5 => {
                self.ticks = 0;
            }
            6 => {
                self.ticks = 0;
                actions.object(GameObjectEvent::PaperChaseShowResult);
                actions.broadcast_text(roster, "paperchase_result", self.matches.to_string());
            }
            _ => return Err(Error::InvalidPluginInput),
        }
        Ok(())
    }
    pub(crate) fn join(&mut self, seat: usize, roster: &Roster) -> Result<Actions, Error> {
        let mut actions = Actions::default();
        actions.text(seat, "paperchase_show", String::new());
        actions.broadcast_text(roster, "paperchase_players", Self::roster(roster));
        actions.items.push(Action::Object {
            target: Target::Seat(seat),
            event: GameObjectEvent::PaperChaseIdle,
        });
        if roster[..3].iter().all(|avatar| *avatar != 0) {
            self.transition(2, roster, &mut actions)?;
        }
        Ok(actions)
    }
    pub(crate) fn message(
        &mut self,
        seat: usize,
        _: &str,
        body: &[u8],
        roster: &Roster,
    ) -> Result<Actions, Error> {
        let mut actions = Actions::default();
        let Some(letter @ 1..=3) = source_integer(body) else {
            return Ok(actions);
        };
        // Native PAPER-PHASE-GUARD: the source UI disables these inputs outside
        // Waiting; its handler can otherwise pre-fill a later round and deadlock.
        if self.phase != 3 {
            return Err(Error::PluginNotReady);
        }
        if self.letters[seat].is_some() {
            return Ok(actions);
        }
        self.letters[seat] = Some(letter as u8);
        actions.object(GameObjectEvent::PaperChaseSetLetter(
            letter | ((seat as i16 + 1) << 8),
        ));
        self.broadcast_letters(roster, &mut actions);
        if self.letters.iter().all(Option::is_some) {
            self.transition(4, roster, &mut actions)?;
        }
        Ok(actions)
    }
    pub(crate) fn tick(&mut self, roster: &Roster) -> Result<Actions, Error> {
        let mut actions = Actions::default();
        match self.phase {
            5 => {
                self.ticks += 1;
                if self.ticks > 420 {
                    self.transition(6, roster, &mut actions)?;
                }
            }
            6 => {
                self.ticks += 1;
                if self.ticks > 90 {
                    self.transition(if self.matches == 3 { 2 } else { 3 }, roster, &mut actions)?;
                }
            }
            _ => {}
        }
        Ok(actions)
    }
    pub(crate) fn leave(&mut self, roster: &Roster) -> Result<Actions, Error> {
        let mut actions = Actions::default();
        actions.broadcast_text(roster, "paperchase_players", Self::roster(roster));
        self.transition(1, roster, &mut actions)?;
        Ok(actions)
    }
    pub(crate) fn rebind(&self, seat: usize, roster: &Roster) -> Actions {
        let mut actions = Actions::default();
        actions.text(seat, "paperchase_show", String::new());
        actions.text(seat, "paperchase_players", Self::roster(roster));
        actions.text(seat, "paperchase_state", self.phase.to_string());
        actions.text(
            seat,
            "paperchase_letters",
            Self::letter_text(&self.displayed),
        );
        if self.phase == 6 {
            actions.text(seat, "paperchase_result", self.matches.to_string());
        }
        actions
    }
    pub(crate) fn save(&self, writer: &mut Writer) {
        writer.u8(1);
        writer.u8(self.phase);
        writer.u16(self.ticks);
        writer.i16(self.matches);
        writer.i16(self.previous_matches);
        for value in self.combination.unwrap_or([0; 3]) {
            writer.u8(value);
        }
        for value in self.letters {
            writer.u8(value.unwrap_or(0));
        }
        for value in self.previous {
            writer.u8(value.unwrap_or(0));
        }
        for value in self.displayed {
            writer.i16(value);
        }
        self.rng.save(writer);
    }
    pub(crate) fn restore(reader: &mut Reader<'_>) -> Result<Self, Error> {
        if reader.u8()? != 1 {
            return Err(Error::InvalidCheckpoint);
        }
        let phase = reader.u8()?;
        let ticks = reader.u16()?;
        let matches = reader.i16()?;
        let previous_matches = reader.i16()?;
        let combination: [u8; 3] = reader
            .take(3)?
            .try_into()
            .map_err(|_| Error::InvalidCheckpoint)?;
        let mut letters = [None; 3];
        let mut previous = [None; 3];
        for values in [&mut letters, &mut previous] {
            for value in values {
                *value = match reader.u8()? {
                    0 => None,
                    value @ 1..=3 => Some(value),
                    _ => return Err(Error::InvalidCheckpoint),
                };
            }
        }
        let mut displayed = [-1; 7];
        for value in &mut displayed {
            *value = reader.i16()?;
        }
        let rng = NativeRng::restore(reader)?;
        if !matches!(phase, 1 | 3 | 5 | 6)
            || !(-1..=3).contains(&matches)
            || !(-1..=2).contains(&previous_matches)
            || displayed[..6]
                .iter()
                .any(|value| !matches!(value, -1 | 1..=3))
            || displayed[6] != previous_matches
            || (phase == 5 && ticks > 420)
            || (phase == 6 && ticks > 90)
            || (phase == 1
                && (ticks != 0
                    || combination != [0; 3]
                    || matches != -1
                    || previous_matches != -1
                    || letters != [None; 3]
                    || previous != [None; 3]
                    || displayed != [-1; 7]))
            || (phase != 1 && combination.iter().any(|value| !(1..=3).contains(value)))
        {
            return Err(Error::InvalidCheckpoint);
        }
        if matches!(phase, 5 | 6)
            && (letters != [None; 3]
                || previous.iter().any(Option::is_none)
                || matches
                    != previous
                        .iter()
                        .zip(combination)
                        .filter(|(actual, wanted)| **actual == Some(*wanted))
                        .count() as i16
                || displayed[..3] != previous.map(|value| i16::from(value.unwrap_or(0))))
        {
            return Err(Error::InvalidCheckpoint);
        }
        if phase == 3 {
            let expected = [
                letters[0].map_or(-1, i16::from),
                letters[1].map_or(-1, i16::from),
                letters[2].map_or(-1, i16::from),
                previous[0].map_or(-1, i16::from),
                previous[1].map_or(-1, i16::from),
                previous[2].map_or(-1, i16::from),
                previous_matches,
            ];
            if displayed != expected
                || matches != previous_matches
                || letters.iter().all(Option::is_some)
                || !matches!(ticks, 0 | 91)
            {
                return Err(Error::InvalidCheckpoint);
            }
        }
        if phase != 1 {
            let last_guess = &displayed[3..6];
            if (previous_matches == -1 && last_guess != [-1; 3])
                || (previous_matches >= 0
                    && (last_guess.contains(&-1)
                        || previous_matches
                            != last_guess
                                .iter()
                                .zip(combination)
                                .filter(|(letter, wanted)| **letter == i16::from(*wanted))
                                .count() as i16))
            {
                return Err(Error::InvalidCheckpoint);
            }
        }
        Ok(Self {
            phase,
            ticks,
            matches,
            previous_matches,
            combination: (phase != 1).then_some(combination),
            letters,
            previous,
            displayed,
            rng,
        })
    }
    pub(crate) fn validate_roster(&self, roster: &Roster) -> bool {
        let full = roster[..3].iter().all(|avatar| *avatar != 0);
        roster[3] == 0 && if self.phase == 1 { !full } else { full }
    }
}
