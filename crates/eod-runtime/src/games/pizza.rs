// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! VMEODPizzaMakerPlugin source translation, including constructor deck tuning.
use super::{rng::NativeRng, *};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PizzaTuning {
    pub phone_wait_seconds: i16,
    pub contribution_timeout_seconds: i16,
    pub restart_delay_seconds: i16,
    pub cards_per_small_ingredient: i16,
    pub cards_per_medium_ingredient: i16,
    pub cards_per_large_ingredient: i16,
    pub cards_per_bonus_ingredient_per_size: i16,
}
impl Default for PizzaTuning {
    fn default() -> Self {
        Self {
            phone_wait_seconds: 5,
            contribution_timeout_seconds: 120,
            restart_delay_seconds: 8,
            cards_per_small_ingredient: 16,
            cards_per_medium_ingredient: 10,
            cards_per_large_ingredient: 8,
            cards_per_bonus_ingredient_per_size: 2,
        }
    }
}
impl PizzaTuning {
    fn values(self) -> [i16; 7] {
        [
            self.phone_wait_seconds,
            self.contribution_timeout_seconds,
            self.restart_delay_seconds,
            self.cards_per_small_ingredient,
            self.cards_per_medium_ingredient,
            self.cards_per_large_ingredient,
            self.cards_per_bonus_ingredient_per_size,
        ]
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Card {
    kind: u8,
    size: u8,
}
impl Card {
    fn text(self) -> String {
        format!("{}{}", self.kind, self.size)
    }
    fn code(self) -> u8 {
        self.kind * 3 + self.size
    }
    fn decode(code: u8) -> Result<Option<Self>, Error> {
        match code {
            0..=17 => Ok(Some(Self {
                kind: code / 3,
                size: code % 3,
            })),
            255 => Ok(None),
            _ => Err(Error::InvalidCheckpoint),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Pizza {
    phase: u8,
    timer: i16,
    frames: u8,
    last_result: u8,
    tuning: PizzaTuning,
    initialized: [bool; 4],
    hands: [[Option<Card>; 3]; 4],
    contributions: [Option<Card>; 4],
    cards: Vec<Card>,
    rng: NativeRng,
}
impl Pizza {
    pub(crate) fn new(seed: u64) -> Result<Self, Error> {
        let mut result = Self {
            phase: 0,
            timer: 0,
            frames: 0,
            last_result: 0,
            tuning: PizzaTuning::default(),
            initialized: [false; 4],
            hands: [[None; 3]; 4],
            contributions: [None; 4],
            cards: Vec::with_capacity(120),
            rng: NativeRng::new(seed),
        };
        // Source PopulateCards executes in the constructor, before any temp1..7
        // tuning is read. Its fixed default pool has exactly 120 cards.
        for kind in 0..3 {
            for (size, count) in [(0, 16), (1, 10), (2, 8)] {
                for _ in 0..count {
                    result.insert(Card { kind, size })?;
                }
            }
            for _ in 0..2 {
                for size in 0..3 {
                    result.insert(Card {
                        kind: kind + 3,
                        size,
                    })?;
                }
            }
        }
        Ok(result)
    }
    fn insert(&mut self, card: Card) -> Result<(), Error> {
        if self.cards.len() >= 120 {
            return Err(Error::InvalidPluginInput);
        }
        let index = self.rng.below(self.cards.len() + 1)?;
        self.cards.insert(index, card);
        Ok(())
    }
    fn take(&mut self) -> Result<Card, Error> {
        if self.cards.is_empty() {
            return Err(Error::InvalidPluginInput);
        }
        Ok(self.cards.remove(0))
    }
    fn roster(roster: &Roster) -> String {
        roster.iter().map(|avatar| format!("{avatar}\n")).collect()
    }
    fn card_text(cards: &[Option<Card>]) -> String {
        cards
            .iter()
            .map(|card| format!("{}\n", card.map_or_else(|| "--".into(), Card::text)))
            .collect()
    }
    fn recipe(contributions: &[Option<Card>; 4]) -> Option<u8> {
        let mut pizza_has: u16 = 0;
        for card in contributions {
            let card = (*card)?;
            pizza_has |= 1 << (card.kind.min(3) + card.size * 4);
        }
        let mut result = 1;
        for size in 0..3 {
            let flags = (pizza_has >> (size * 4)) & 15;
            if flags == 15 {
                result = size + 5;
            } else if flags == 7 {
                result = size + 2;
            }
        }
        Some(result)
    }
    fn time(&mut self, timer: i16, roster: &Roster, actions: &mut Actions) {
        self.timer = timer;
        self.frames = 0;
        actions.broadcast_text(roster, "pizza_time", timer.to_string());
    }
    fn enter(&mut self, phase: u8, roster: &Roster, actions: &mut Actions) -> Result<(), Error> {
        self.phase = phase;
        actions.broadcast_text(roster, "pizza_state", phase.to_string());
        match phase {
            0 => {
                self.contributions = [None; 4];
                actions.object(GameObjectEvent::PizzaRestart);
                self.time(-1, roster, actions);
                actions.broadcast_text(roster, "pizza_players", Self::roster(roster));
            }
            1 => {
                for seat in 0..4 {
                    for slot in 0..3 {
                        if self.hands[seat][slot].is_none() {
                            self.hands[seat][slot] = Some(self.take()?);
                        }
                    }
                    actions.text(seat, "pizza_hand", Self::card_text(&self.hands[seat]));
                }
                self.time(self.tuning.phone_wait_seconds, roster, actions);
            }
            2 => self.time(self.tuning.contribution_timeout_seconds, roster, actions),
            3 => {
                self.time(-1, roster, actions);
                for seat in 0..4 {
                    if self.contributions[seat].is_none() {
                        let chosen = self.rng.below(3)?;
                        let card = self.hands[seat][chosen].ok_or(Error::InvalidPluginInput)?;
                        self.contributions[seat] = Some(card);
                        self.insert(card)?;
                        actions.object(GameObjectEvent::PizzaContribute(
                            i16::from(card.kind) | ((seat as i16) << 8),
                        ));
                        self.hands[seat][chosen] = None;
                        actions.text(seat, "pizza_hand", Self::card_text(&self.hands[seat]));
                    }
                }
                actions.broadcast_text(
                    roster,
                    "pizza_contrib",
                    Self::card_text(&self.contributions),
                );
                let result = Self::recipe(&self.contributions).ok_or(Error::InvalidPluginInput)?;
                self.last_result = result;
                actions.object(GameObjectEvent::PizzaBake(i16::from(result)));
            }
            4 => {
                actions.broadcast_text(roster, "pizza_result", self.last_result.to_string());
                actions.object(GameObjectEvent::PizzaPayoutResult(i16::from(
                    self.last_result,
                )));
                self.time(self.tuning.restart_delay_seconds, roster, actions);
            }
            _ => return Err(Error::InvalidPluginInput),
        }
        Ok(())
    }
    pub(crate) fn join(
        &mut self,
        seat: usize,
        tuning: PizzaTuning,
        roster: &Roster,
    ) -> Result<Actions, Error> {
        self.tuning = tuning;
        self.initialized[seat] = true;
        let mut actions = Actions::default();
        actions.text(seat, "pizza_show", String::new());
        actions.broadcast_text(roster, "pizza_players", Self::roster(roster));
        Ok(actions)
    }
    pub(crate) fn message(
        &mut self,
        seat: usize,
        event: &str,
        body: &[u8],
        roster: &Roster,
    ) -> Result<Actions, Error> {
        let mut actions = Actions::default();
        // The source explicitly comments out Server.Disconnect in P_Close.
        if event == "close" {
            return Ok(actions);
        }
        let Some(item @ 0..=2) = source_integer(body) else {
            return Ok(actions);
        };
        if self.phase != 2 || self.contributions[seat].is_some() {
            return Ok(actions);
        }
        let item = item as usize;
        let card = self.hands[seat][item].ok_or(Error::InvalidPluginInput)?;
        self.contributions[seat] = Some(card);
        actions.object(GameObjectEvent::PizzaContribute(
            i16::from(card.kind) | ((seat as i16) << 8),
        ));
        self.insert(card)?;
        self.hands[seat][item] = None;
        actions.broadcast_text(
            roster,
            "pizza_contrib",
            Self::card_text(&self.contributions),
        );
        Ok(actions)
    }
    pub(crate) fn tick(&mut self, roster: &Roster) -> Result<Actions, Error> {
        let mut actions = Actions::default();
        if self.timer > 0 {
            self.frames += 1;
            if self.frames >= 30 {
                self.frames = 0;
                self.timer -= 1;
                actions.broadcast_text(roster, "pizza_time", self.timer.to_string());
            }
        }
        match self.phase {
            0 if roster.iter().all(|avatar| *avatar != 0) => self.enter(1, roster, &mut actions)?,
            1 if self.timer == 0 => {
                actions.object(GameObjectEvent::PizzaRingPhone(roster[1]));
                self.timer = -1;
            }
            2 if self.timer == 0 => {
                self.timer = -1;
                self.enter(3, roster, &mut actions)?;
            }
            4 if self.timer == 0 => {
                self.timer = -1;
                self.enter(0, roster, &mut actions)?;
            }
            _ => {}
        }
        Ok(actions)
    }
    pub(crate) fn vm_event(
        &mut self,
        event: GameVmInput,
        roster: &Roster,
    ) -> Result<Actions, Error> {
        let mut actions = Actions::default();
        match (event, self.phase) {
            (GameVmInput::PizzaRespondPhone, 1) => self.enter(2, roster, &mut actions)?,
            (GameVmInput::PizzaAllContributed, 2) => self.enter(3, roster, &mut actions)?,
            (GameVmInput::PizzaRespondBake, 3) => self.enter(4, roster, &mut actions)?,
            _ => {}
        }
        Ok(actions)
    }
    pub(crate) fn leave(&mut self, seat: usize, roster: &Roster) -> Result<Actions, Error> {
        self.contributions[seat] = None;
        let mut actions = Actions::default();
        if self.phase != 0 {
            self.enter(0, roster, &mut actions)?;
        } else {
            actions.broadcast_text(roster, "pizza_players", Self::roster(roster));
        }
        Ok(actions)
    }
    pub(crate) fn rebind(&self, seat: usize, roster: &Roster) -> Actions {
        let mut actions = Actions::default();
        actions.text(seat, "pizza_show", String::new());
        actions.text(seat, "pizza_players", Self::roster(roster));
        actions.text(seat, "pizza_state", self.phase.to_string());
        actions.text(seat, "pizza_time", self.timer.to_string());
        actions.text(seat, "pizza_hand", Self::card_text(&self.hands[seat]));
        actions.text(seat, "pizza_contrib", Self::card_text(&self.contributions));
        if self.phase == 4 {
            actions.text(seat, "pizza_result", self.last_result.to_string());
        }
        actions
    }
    pub(crate) fn save(&self, writer: &mut Writer) {
        writer.u8(1);
        writer.u8(self.phase);
        writer.i16(self.timer);
        writer.u8(self.frames);
        writer.u8(self.last_result);
        for value in self.tuning.values() {
            writer.i16(value);
        }
        for seat in 0..4 {
            writer.bool(self.initialized[seat]);
            for card in self.hands[seat] {
                writer.u8(card.map_or(255, Card::code));
            }
            writer.u8(self.contributions[seat].map_or(255, Card::code));
        }
        writer.u8(self.cards.len() as u8);
        for card in &self.cards {
            writer.u8(card.code());
        }
        self.rng.save(writer);
    }
    pub(crate) fn restore(reader: &mut Reader<'_>) -> Result<Self, Error> {
        if reader.u8()? != 1 {
            return Err(Error::InvalidCheckpoint);
        }
        let phase = reader.u8()?;
        let timer = reader.i16()?;
        let frames = reader.u8()?;
        let last_result = reader.u8()?;
        let tuning = PizzaTuning {
            phone_wait_seconds: reader.i16()?,
            contribution_timeout_seconds: reader.i16()?,
            restart_delay_seconds: reader.i16()?,
            cards_per_small_ingredient: reader.i16()?,
            cards_per_medium_ingredient: reader.i16()?,
            cards_per_large_ingredient: reader.i16()?,
            cards_per_bonus_ingredient_per_size: reader.i16()?,
        };
        let mut initialized = [false; 4];
        let mut hands = [[None; 3]; 4];
        let mut contributions = [None; 4];
        for seat in 0..4 {
            initialized[seat] = reader.bool()?;
            for card in &mut hands[seat] {
                *card = Card::decode(reader.u8()?)?;
            }
            contributions[seat] = Card::decode(reader.u8()?)?;
        }
        let count = usize::from(reader.u8()?);
        if !(108..=120).contains(&count) {
            return Err(Error::InvalidCheckpoint);
        }
        let mut cards = Vec::with_capacity(120);
        for _ in 0..count {
            cards.push(Card::decode(reader.u8()?)?.ok_or(Error::InvalidCheckpoint)?);
        }
        let rng = NativeRng::restore(reader)?;
        let mut pool_counts = [0u16; 18];
        for card in &cards {
            pool_counts[usize::from(card.code())] += 1;
        }
        let mut counts = pool_counts;
        for card in hands.iter().flatten().flatten() {
            counts[usize::from(card.code())] += 1;
        }
        let mut returned_counts = [0u16; 18];
        for card in contributions.iter().flatten() {
            returned_counts[usize::from(card.code())] += 1;
        }
        let expected = [16, 10, 8, 16, 10, 8, 16, 10, 8, 2, 2, 2, 2, 2, 2, 2, 2, 2];
        let countdown_reachable = |setting: i16| {
            if setting > 0 {
                (1..=setting).contains(&timer)
            } else {
                timer == setting
            }
        };
        // Countdown zero transitions in the same tick, except the initial zero
        // supplied by tuning. Phone keeps -1 after its ring event. Preserve
        // negative source tuning, whose countdown deliberately never advances.
        let timer_reachable = match phase {
            0 => matches!(timer, -1 | 0),
            1 => {
                countdown_reachable(tuning.phone_wait_seconds)
                    || (tuning.phone_wait_seconds >= 0 && timer == -1)
            }
            2 => countdown_reachable(tuning.contribution_timeout_seconds),
            3 => timer == -1,
            4 => countdown_reachable(tuning.restart_delay_seconds),
            _ => false,
        };
        if phase > 4
            || frames >= 30
            || last_result > 7
            || counts != expected
            || !timer_reachable
            || (timer <= 0 && frames != 0)
            || pool_counts
                .iter()
                .zip(returned_counts)
                .any(|(available, returned)| *available < returned)
            || ((phase == 0 || phase == 1) && contributions != [None; 4])
            || (phase >= 3 && Self::recipe(&contributions) != Some(last_result))
            || (phase == 3 && timer != -1)
            || (phase != 0 && initialized != [true; 4])
        {
            return Err(Error::InvalidCheckpoint);
        }
        for seat in 0..4 {
            if (!initialized[seat] && (hands[seat] != [None; 3] || contributions[seat].is_some()))
                || (contributions[seat].is_some()
                    && hands[seat].iter().filter(|card| card.is_none()).count() != 1)
                || (phase != 0
                    && contributions[seat].is_none()
                    && hands[seat].iter().any(Option::is_none))
            {
                return Err(Error::InvalidCheckpoint);
            }
        }
        Ok(Self {
            phase,
            timer,
            frames,
            last_result,
            tuning,
            initialized,
            hands,
            contributions,
            cards,
            rng,
        })
    }
    pub(crate) fn validate_roster(&self, roster: &Roster) -> bool {
        (self.phase == 0 || roster.iter().all(|avatar| *avatar != 0))
            && roster
                .iter()
                .enumerate()
                .all(|(seat, avatar)| *avatar == 0 || self.initialized[seat])
    }
}
