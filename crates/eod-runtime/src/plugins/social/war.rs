// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
use super::{util::*, *};

const LOBBY: u8 = 0;
const CHOOSING: u8 = 1;
const ANIMATING: u8 = 2;
const ROUND_TIE: u8 = 3;
const GAME_TIE: u8 = 4;
const GAME_OVER: u8 = 5;

#[derive(Clone)]
pub(super) struct War {
    pieces: [u8; 2],
    chosen: [Option<u8>; 2],
    phase: u8,
    wait: u16,
}
impl War {
    pub(super) fn new() -> Self {
        Self {
            pieces: [31, 31],
            chosen: [None; 2],
            phase: LOBBY,
            wait: 0,
        }
    }
    pub(super) fn join(&mut self, member: Member, roster: &Roster) -> Result<Actions, Error> {
        if !valid_roles(roster, 2) {
            return Err(Error::InvalidPluginInput);
        }
        let mut a = self.view(member, roster);
        if full(roster, 2) {
            role_text(
                &mut a,
                roster,
                0,
                "WarGame_Draw_Opponent",
                by_role(roster, 1)
                    .ok_or(Error::InvalidPluginInput)?
                    .avatar_object
                    .to_string(),
            );
            role_text(
                &mut a,
                roster,
                1,
                "WarGame_Draw_Opponent",
                by_role(roster, 0)
                    .ok_or(Error::InvalidPluginInput)?
                    .avatar_object
                    .to_string(),
            );
            append(&mut a, self.next_game(roster)?);
        }
        Ok(a)
    }
    pub(super) fn view(&self, member: Member, _roster: &Roster) -> Actions {
        let mut a = Actions::default();
        let color = if member.input.role == 0 {
            "blue"
        } else {
            "red"
        };
        a.text(
            Target::Member(member.seat),
            "WarGame_Init",
            format!("{}%{color}", member.avatar_object),
        );
        a
    }
    pub(super) fn rebind(&self, member: Member, roster: &Roster) -> Actions {
        let mut a = self.view(member, roster);
        let opponent = by_role(roster, 1 - member.input.role)
            .map(|m| m.avatar_object.to_string())
            .unwrap_or_default();
        a.text(
            Target::Member(member.seat),
            "WarGame_Draw_Opponent",
            opponent,
        );
        // Recreate removed buttons using the exact source Defeat payload. The
        // UI chooses the defeated piece according to its native role.
        a.text(Target::Member(member.seat), "WarGame_Reset", String::new());
        for piece in 0..5 {
            if self.pieces[usize::from(member.input.role)] & (1 << piece) == 0 {
                let mut pair = [0; 2];
                pair[usize::from(member.input.role)] = piece;
                a.binary(Target::Member(member.seat), "WarGame_Defeat", pair.to_vec());
            }
        }
        if self.phase == CHOOSING && self.chosen[usize::from(member.input.role)].is_none() {
            a.binary(Target::Member(member.seat), "WarGame_Resume", self.counts());
        }
        a
    }
    fn counts(&self) -> Vec<u8> {
        self.pieces.map(|pieces| pieces.count_ones() as u8).to_vec()
    }
    fn next_game(&mut self, roster: &Roster) -> Result<Actions, Error> {
        if !full(roster, 2) {
            return Err(Error::PluginNotReady);
        }
        self.pieces = [31, 31];
        self.chosen = [None; 2];
        self.wait = 0;
        let mut a = Actions::default();
        a.text(Target::All, "WarGame_Reset", String::new());
        append(&mut a, self.next_round(roster)?);
        Ok(a)
    }
    fn next_round(&mut self, roster: &Roster) -> Result<Actions, Error> {
        if !full(roster, 2) {
            return Err(Error::PluginNotReady);
        }
        let mut a = Actions::default();
        self.chosen = [None; 2];
        self.wait = 0;
        if self.pieces.contains(&0) {
            let winner = if self.pieces[0].count_ones() > self.pieces[1].count_ones() {
                0
            } else {
                1
            };
            a.object(Target::Controller, 2, vec![winner]);
            self.pieces = [31, 31];
            self.phase = GAME_OVER;
        } else if self.pieces[0].count_ones() == 1 && self.pieces[0] == self.pieces[1] {
            a.binary(
                Target::All,
                "WarGame_Stalemate",
                vec![self.pieces[0].trailing_zeros() as u8],
            );
            self.phase = GAME_TIE;
            self.wait = 150;
        } else {
            // WAR-LAST-PIECE-PROGRESS: unequal final pieces are still a legal
            // deciding round. The original nested branch omitted Resume.
            a.binary(Target::All, "WarGame_Resume", self.counts());
            self.phase = CHOOSING;
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
        if event == "WarGame_Close_UI" {
            a.close(Target::Member(member.seat));
            return Ok(a);
        }
        if event != "WarGame_Piece_Selection" {
            return Err(Error::EventNotAllowed);
        }
        if self.phase != CHOOSING || !full(roster, 2) {
            return Err(Error::PluginNotReady);
        }
        let piece = *payload.first().ok_or(Error::InvalidMessage)?;
        let player = usize::from(member.input.role);
        let available = self.pieces[player];
        if available == 0 {
            a.close(Target::Member(member.seat));
            return Ok(a);
        }
        let chosen = if piece < 5 && available & (1 << piece) != 0 {
            piece
        } else {
            available.trailing_zeros() as u8
        };
        self.chosen[player] = Some(chosen);
        if let [Some(blue), Some(red)] = self.chosen {
            let pair = vec![blue, red];
            if let Some(loser) = super::war_rules::defeated_player(blue, red) {
                self.pieces[usize::from(loser)] &= !(1 << pair[usize::from(loser)]);
                role_binary(&mut a, roster, 1 - loser, "WarGame_Victory", pair.clone());
                role_binary(&mut a, roster, loser, "WarGame_Defeat", pair);
                a.object(Target::Controller, 1, vec![i16::from(1 - loser)]);
                self.phase = ANIMATING;
            } else {
                a.binary(Target::All, "WarGame_Tie", pair);
                self.phase = ROUND_TIE;
                self.wait = 150;
            }
            self.chosen = [None; 2];
        }
        Ok(a)
    }
    pub(super) fn tick(&mut self, roster: &Roster) -> Result<Actions, Error> {
        if self.wait == 0 {
            return Ok(Actions::default());
        }
        if !full(roster, 2) {
            return Err(Error::PluginNotReady);
        }
        self.wait -= 1;
        if self.wait != 0 {
            return Ok(Actions::default());
        }
        if self.phase == GAME_TIE {
            let mut a = Actions::default();
            a.object(Target::Controller, 3, Vec::new());
            append(&mut a, self.next_game(roster)?);
            Ok(a)
        } else {
            self.next_round(roster)
        }
    }
    pub(super) fn vm_event(&mut self, input: &VmInput, roster: &Roster) -> Result<Actions, Error> {
        match input {
            VmInput::WarNextRound if self.phase == ANIMATING => self.next_round(roster),
            VmInput::WarNextGame if full(roster, 2) => self.next_game(roster),
            VmInput::WarNextRound | VmInput::WarNextGame => Err(Error::PluginNotReady),
            _ => Err(Error::InvalidPluginInput),
        }
    }
    pub(super) fn leave(&mut self) -> Actions {
        *self = Self::new();
        let mut a = Actions::default();
        a.text(Target::All, "WarGame_Reset", String::new());
        a.text(Target::All, "WarGame_Draw_Opponent", String::new());
        a
    }
    pub(super) fn validate(&self, roster: &Roster) -> bool {
        valid_roles(roster, 2)
            && self.phase <= GAME_OVER
            && self.pieces.iter().all(|p| *p <= 31)
            && self.pieces.iter().any(|p| *p != 0)
            && self.chosen.iter().enumerate().all(|(i, c)| {
                c.is_none_or(|c| c < 5 && self.pieces[i] & (1 << c) != 0 && self.phase == CHOOSING)
            })
            && self.chosen.iter().filter(|c| c.is_some()).count() <= 1
            && if self.phase == LOBBY {
                !full(roster, 2) && self.wait == 0 && self.pieces == [31, 31]
            } else {
                full(roster, 2)
                    && match self.phase {
                        ROUND_TIE | GAME_TIE => {
                            self.wait > 0
                                && self.wait <= 150
                                && (self.phase != GAME_TIE
                                    || self.pieces[0].count_ones() == 1
                                        && self.pieces[0] == self.pieces[1])
                        }
                        CHOOSING => self.wait == 0 && !self.pieces.contains(&0),
                        GAME_OVER => self.wait == 0 && self.pieces == [31, 31],
                        _ => self.wait == 0,
                    }
            }
    }
    pub(super) fn save(&self, w: &mut Writer) {
        w.fixed(&self.pieces);
        for chosen in self.chosen {
            w.u8(chosen.unwrap_or(255));
        }
        w.u8(self.phase);
        w.u16(self.wait);
    }
    pub(super) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let pieces = [r.u8()?, r.u8()?];
        let mut chosen = [None; 2];
        for item in &mut chosen {
            let value = r.u8()?;
            if value != 255 {
                if value > 4 {
                    return Err(Error::InvalidCheckpoint);
                }
                *item = Some(value);
            }
        }
        Ok(Self {
            pieces,
            chosen,
            phase: r.u8()?,
            wait: r.u16()?,
        })
    }
}
