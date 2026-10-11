// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
use super::{util::*, *};

pub(super) const REQUEST: u16 = 200;
pub(super) const VIEW: u16 = 201;
pub(super) const RESET: u16 = 202;
pub(super) const FLOOR_READY: u16 = 203;
pub(super) const PARTICLE: u16 = 204;
pub(super) const GOOD_PATTERN: u16 = 205;
pub(super) const DETACH: u16 = 206;
const MAX_SECTIONS: usize = 128;

#[derive(Clone)]
pub(super) struct Freshness {
    amount: f32,
    history: Vec<Vec<i32>>,
}
impl Freshness {
    fn new(categories: usize) -> Self {
        Self {
            amount: 0.0,
            history: vec![vec![-1; 4]; categories],
        }
    }
    fn tick(&mut self) {
        self.amount -= (1.0f32 / 900.0) * self.amount;
        if self.amount < 0.0 {
            self.amount = 0.0;
        }
    }
    fn send(&mut self, command: i32, category: usize) {
        let history = &mut self.history[category];
        if let Some(index) = history.iter().position(|old| *old == command) {
            history.remove(index);
            history.push(command);
            self.amount += 0.15 * ((3 - index) as f32 / 4.0);
        } else {
            if history.len() > 3 {
                history.remove(0);
            }
            // Intentionally preserve the source's missing Add in this branch.
            self.amount += 0.15;
        }
        if self.amount > 1.1 {
            self.amount = 1.1;
        }
    }
    fn freshness(&self) -> f32 {
        self.amount.min(1.0)
    }
    fn good(&self) -> f32 {
        0.75 + self.freshness() * 0.25
    }
    fn valid(&self, categories: usize) -> bool {
        self.amount.is_finite()
            && (0.0..=1.1).contains(&self.amount)
            && self.history.len() == categories
            && self
                .history
                .iter()
                .all(|h| (3..=4).contains(&h.len()) && h.iter().all(|v| *v == -1))
    }
    fn save(&self, w: &mut Writer) {
        w.u32(self.amount.to_bits());
        w.u8(self.history.len() as u8);
        for history in &self.history {
            w.u8(history.len() as u8);
            for v in history {
                w.i32(*v);
            }
        }
    }
    fn restore(r: &mut Reader<'_>, categories: usize) -> Result<Self, Error> {
        let amount = f32::from_bits(r.u32()?);
        let count = r.u8()?;
        if usize::from(count) != categories {
            return Err(Error::InvalidCheckpoint);
        }
        let mut history = Vec::with_capacity(categories);
        for _ in 0..count {
            let count = r.u8()?;
            if !(3..=4).contains(&count) {
                return Err(Error::InvalidCheckpoint);
            }
            let mut h = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                h.push(r.i32()?);
            }
            history.push(h);
        }
        let result = Self { amount, history };
        if !result.valid(categories) {
            return Err(Error::InvalidCheckpoint);
        }
        Ok(result)
    }
}

#[derive(Clone)]
pub(super) struct View {
    controller: Option<InstanceId>,
    active: bool,
    ticks: i32,
    section: u8,
    dances: [u8; 3],
    dj: [u8; 4],
}
impl View {
    fn new() -> Self {
        Self {
            controller: None,
            active: false,
            ticks: -1,
            section: 0,
            dances: [0; 3],
            dj: [0; 4],
        }
    }
    fn percentage(&self) -> f32 {
        self.ticks as f32 / 9000.0
    }
    fn update(&mut self, source: &PeerSource, signal: &Signal) -> Result<(), Error> {
        if source.plugin != NIGHTCLUB || signal.numbers.len() != 9 || !signal.bytes.is_empty() {
            return Err(Error::InvalidPluginInput);
        }
        if self
            .controller
            .is_some_and(|controller| controller != source.group)
        {
            return Err(Error::InvalidPluginInput);
        }
        self.controller = Some(source.group);
        self.ticks = i32::try_from(signal.numbers[0]).map_err(|_| Error::InvalidPluginInput)?;
        self.active = self.ticks >= 0;
        self.section = u8::try_from(signal.numbers[1]).map_err(|_| Error::InvalidPluginInput)?;
        for (target, value) in self.dances.iter_mut().zip(&signal.numbers[2..5]) {
            *target = u8::try_from(*value).map_err(|_| Error::InvalidPluginInput)?;
        }
        for (target, value) in self.dj.iter_mut().zip(&signal.numbers[5..9]) {
            *target = u8::try_from(*value).map_err(|_| Error::InvalidPluginInput)?;
        }
        if !self.valid() {
            return Err(Error::InvalidPluginInput);
        }
        Ok(())
    }
    fn valid(&self) -> bool {
        (-1..=(MAX_SECTIONS as i32 * 1800)).contains(&self.ticks)
            && usize::from(self.section) < MAX_SECTIONS
            && self.active == (self.ticks >= 0)
            && (self.controller.is_some() || !self.active && self.ticks == -1)
            && self.dances.iter().all(|v| *v < 24)
            && self.dj.iter().all(|v| *v < 64)
            && (self.ticks <= 0
                || self.dances[0] != self.dances[1]
                    && self.dances[0] != self.dances[2]
                    && self.dances[1] != self.dances[2])
    }
    fn validate_peers(&self, cluster: u64, peers: &[PeerGroup]) -> bool {
        self.controller.is_none_or(|group| {
            linked_peer(peers, group, cluster, NIGHTCLUB)
                .is_some_and(|peer| peer.roster.iter().all(Option::is_none))
        })
    }
    fn save(&self, w: &mut Writer) {
        save_option_id(self.controller, w);
        w.bool(self.active);
        w.i32(self.ticks);
        w.u8(self.section);
        w.fixed(&self.dances);
        w.fixed(&self.dj);
    }
    fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let value = Self {
            controller: restore_option_id(r)?,
            active: r.bool()?,
            ticks: r.i32()?,
            section: r.u8()?,
            dances: r
                .take(3)?
                .try_into()
                .map_err(|_| Error::InvalidCheckpoint)?,
            dj: r
                .take(4)?
                .try_into()
                .map_err(|_| Error::InvalidCheckpoint)?,
        };
        if !value.valid() {
            return Err(Error::InvalidCheckpoint);
        }
        Ok(value)
    }
}

#[derive(Clone)]
pub(super) struct Controller {
    rng: Rng,
    portals: Vec<i16>,
    dancers: Vec<i16>,
    dancer_index: usize,
    ticks: i32,
    section: u8,
    dances: [u8; 3],
    dj: [u8; 4],
    floor: Option<InstanceId>,
}
impl Controller {
    pub(super) fn required_peers(&self) -> Vec<InstanceId> {
        self.floor.into_iter().collect()
    }

    pub(super) fn new(seed: Seed, portals: Vec<i16>) -> Result<Self, Error> {
        if !valid_objects(&portals, 4096) {
            return Err(Error::InvalidPluginInput);
        }
        Ok(Self {
            rng: Rng::new(seed.0),
            portals,
            dancers: Vec::new(),
            dancer_index: 0,
            ticks: -1,
            section: 0,
            dances: [0; 3],
            dj: [0; 4],
            floor: None,
        })
    }
    pub(super) fn start(&self) -> Actions {
        let mut a = Actions::default();
        if !self.portals.is_empty() {
            a.command(NativeCommand::BatchGraphics {
                objects: self.portals.clone(),
                graphics: vec![255; self.portals.len()],
            });
        }
        a.peer(NC_FLOOR, signal(REQUEST, Vec::new(), Vec::new()));
        a
    }
    fn broadcast(&self, a: &mut Actions) {
        let mut numbers = vec![i64::from(self.ticks), i64::from(self.section)];
        numbers.extend(self.dances.map(i64::from));
        numbers.extend(self.dj.map(i64::from));
        for plugin in [DJ_STATION, DANCE_PLATFORM] {
            a.peer(plugin, signal(VIEW, numbers.clone(), Vec::new()));
        }
    }
    pub(super) fn tick(&mut self) -> Result<Actions, Error> {
        let mut a = Actions::default();
        if self.floor.is_none() || self.ticks < 0 {
            return Ok(a);
        }
        if self.ticks >= MAX_SECTIONS as i32 * 1800 {
            return Err(Error::CounterExhausted);
        }
        let cycle = self.ticks % 1800;
        if cycle == 0 {
            // Later choices exclude both already updated entries and the old
            // not-yet-updated entries, exactly as the source's Any predicate.
            for index in 0..3 {
                let mut chosen = None;
                for _ in 0..64 {
                    let value = self.rng.below(24)? as u8;
                    if !self.dances.contains(&value) {
                        chosen = Some(value);
                        break;
                    }
                }
                self.dances[index] = chosen.ok_or(Error::InvalidPluginInput)?;
            }
            if self.ticks != 0 {
                self.section = self
                    .section
                    .checked_add(1)
                    .filter(|s| usize::from(*s) < MAX_SECTIONS)
                    .ok_or(Error::CounterExhausted)?;
            }
        }
        if self.ticks % (5 + (cycle / 30).min(25)) == 0 && !self.dancers.is_empty() {
            let roll = self.rng.below(100)?;
            let slot = if roll < 20 {
                0
            } else if roll < 55 {
                1
            } else {
                2
            };
            let dancer = self.dancers[self.dancer_index];
            self.dancer_index = (self.dancer_index + 1) % self.dancers.len();
            a.command(NativeCommand::ForceInteraction {
                caller: dancer,
                callee: dancer,
                interaction: u16::from(self.dances[slot]) + 4,
            });
        }
        self.ticks += 1;
        self.broadcast(&mut a);
        Ok(a)
    }
    pub(super) fn vm_event(&mut self, input: &VmInput) -> Result<Actions, Error> {
        let mut a = Actions::default();
        match input {
            VmInput::NightclubRoundStart { dancers } => {
                if !valid_objects(dancers, 128) {
                    return Err(Error::InvalidPluginInput);
                }
                self.section = 0;
                self.ticks = 0;
                self.dancer_index = 0;
                for pattern in &mut self.dj {
                    *pattern = self.rng.below(64)? as u8;
                }
                self.dancers.clear();
                for dancer in dancers {
                    let index = self.rng.below(self.dancers.len() as u32 + 1)? as usize;
                    self.dancers.insert(index, *dancer);
                }
                for plugin in [DJ_STATION, DANCE_PLATFORM] {
                    a.peer(plugin, signal(RESET, Vec::new(), Vec::new()));
                }
                self.broadcast(&mut a);
            }
            VmInput::NightclubRoundEnd => {
                self.ticks = -1;
                self.broadcast(&mut a);
            }
            _ => return Err(Error::InvalidPluginInput),
        }
        Ok(a)
    }
    pub(super) fn peer(&mut self, source: &PeerSource, signal: &Signal) -> Result<Actions, Error> {
        let mut a = Actions::default();
        if source.plugin == NC_FLOOR {
            if signal.code == FLOOR_READY {
                if self.floor.is_some_and(|floor| floor != source.group) {
                    return Err(Error::InvalidPluginInput);
                }
                let newly_discovered = self.floor.is_none();
                self.floor = Some(source.group);
                if newly_discovered {
                    // The floor can appear after this controller's first
                    // discovery request. Complete the reciprocal binding once,
                    // so a later controller cannot claim the same floor.
                    a.peer(NC_FLOOR, util::signal(REQUEST, Vec::new(), Vec::new()));
                }
            } else if signal.code == DETACH && self.floor == Some(source.group) {
                self.floor = None;
            }
        } else if [DJ_STATION, DANCE_PLATFORM].contains(&source.plugin) && signal.code == REQUEST {
            self.broadcast(&mut a);
        }
        Ok(a)
    }
    pub(super) fn shutdown(&mut self) -> Actions {
        self.ticks = -1;
        let mut a = Actions::default();
        self.broadcast(&mut a);
        for plugin in [DJ_STATION, DANCE_PLATFORM, NC_FLOOR] {
            a.peer(plugin, signal(DETACH, Vec::new(), Vec::new()));
        }
        a
    }
    pub(super) fn validate(&self, roster: &Roster) -> bool {
        roster.iter().all(Option::is_none)
            && valid_objects(&self.portals, 4096)
            && valid_objects(&self.dancers, 128)
            && self.dancer_index < self.dancers.len().max(1)
            && (-1..=(MAX_SECTIONS as i32 * 1800)).contains(&self.ticks)
            && usize::from(self.section) < MAX_SECTIONS
            && self.dances.iter().all(|v| *v < 24)
            && self.dj.iter().all(|v| *v < 64)
            && (self.ticks <= 0
                || self.dances[0] != self.dances[1]
                    && self.dances[0] != self.dances[2]
                    && self.dances[1] != self.dances[2])
    }
    pub(super) fn validate_peers(&self, cluster: u64, peers: &[PeerGroup]) -> bool {
        self.floor.is_none_or(|group| {
            linked_peer(peers, group, cluster, NC_FLOOR)
                .is_some_and(|peer| valid_single(&peer.roster))
        })
    }
    pub(super) fn save(&self, w: &mut Writer) {
        self.rng.save(w);
        save_objects(&self.portals, w);
        save_objects(&self.dancers, w);
        w.u16(self.dancer_index as u16);
        w.i32(self.ticks);
        w.u8(self.section);
        w.fixed(&self.dances);
        w.fixed(&self.dj);
        save_option_id(self.floor, w);
    }
    pub(super) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(Self {
            rng: Rng::restore(r)?,
            portals: restore_objects(r, 4096)?,
            dancers: restore_objects(r, 128)?,
            dancer_index: usize::from(r.u16()?),
            ticks: r.i32()?,
            section: r.u8()?,
            dances: r
                .take(3)?
                .try_into()
                .map_err(|_| Error::InvalidCheckpoint)?,
            dj: r
                .take(4)?
                .try_into()
                .map_err(|_| Error::InvalidCheckpoint)?,
            floor: restore_option_id(r)?,
        })
    }
}

#[derive(Clone)]
pub(super) struct Dj {
    rng: Rng,
    group: u8,
    x: i16,
    y: i16,
    fresh: Freshness,
    patterns: [[u8; 3]; 4],
    dirty: [bool; 4],
    correct: [bool; 4],
    ratings: [f32; 4],
    last_rating: i16,
    rating: i16,
    time: i16,
    club: View,
}
impl Dj {
    pub(super) fn required_peers(&self) -> Vec<InstanceId> {
        self.club.controller.into_iter().collect()
    }

    pub(super) fn new(seed: Seed, group: u8, x: i16, y: i16) -> Result<Self, Error> {
        if group > 3 {
            return Err(Error::InvalidPluginInput);
        }
        Ok(Self {
            rng: Rng::new(seed.0),
            group,
            x,
            y,
            fresh: Freshness::new(4),
            patterns: [[0; 3]; 4],
            dirty: [false; 4],
            correct: [false; 4],
            ratings: [0.0; 4],
            last_rating: -1,
            rating: 0,
            time: 0,
            club: View::new(),
        })
    }
    pub(super) fn start(&self) -> Actions {
        let mut a = Actions::default();
        a.peer(NIGHTCLUB, signal(REQUEST, Vec::new(), Vec::new()));
        a
    }
    fn patterns(&self) -> String {
        self.patterns
            .iter()
            .map(|p| p.iter().map(|v| char::from(b'0' + *v)).collect::<String>())
            .collect::<Vec<_>>()
            .join("|")
    }
    pub(super) fn view(&self, member: Member) -> Actions {
        let mut a = Actions::default();
        a.text(
            Target::Member(member.seat),
            "dj_show",
            self.group.to_string(),
        );
        a.text(Target::Member(member.seat), "dj_active", self.patterns());
        a
    }
    fn reset(&mut self) -> Result<Actions, Error> {
        self.fresh = Freshness::new(4);
        for pattern in &mut self.patterns {
            for digit in pattern {
                *digit = self.rng.below(4)? as u8;
            }
        }
        self.dirty = [true; 4];
        self.ratings = [0.0; 4];
        self.correct = [false; 4];
        self.rating = 0;
        self.last_rating = -1;
        self.time = i16::from(self.group) * 45;
        let mut a = Actions::default();
        a.text(Target::All, "dj_active", self.patterns());
        Ok(a)
    }
    pub(super) fn message(
        &mut self,
        member: Member,
        event: &str,
        payload: &[u8],
    ) -> Result<Actions, Error> {
        let mut a = Actions::default();
        if event == "close" {
            a.close(Target::Member(member.seat));
            return Ok(a);
        }
        if event != "press_button" {
            return Err(Error::EventNotAllowed);
        }
        if payload.len() < 3 {
            return Ok(a);
        }
        let p = &payload[..3];
        // Source checks <4 for the three-digit array index and then throws on
        // index 3. This native bounds guard rejects that malformed message.
        if !(b'0'..=b'3').contains(&p[0])
            || !(b'0'..=b'2').contains(&p[1])
            || !(b'0'..=b'3').contains(&p[2])
        {
            return Ok(a);
        }
        let category = usize::from(p[0] - b'0');
        self.dirty[category] = true;
        self.patterns[category][usize::from(p[1] - b'0')] = p[2] - b'0';
        let pattern = self.patterns[category];
        let index = i16::from(pattern[0]) * 16 + i16::from(pattern[1]) * 4 + i16::from(pattern[2]);
        let corrected_category = match category {
            0 => 1,
            1 => 0,
            value => value,
        };
        a.object(
            Target::Controller,
            10 + corrected_category as i16,
            vec![index],
        );
        a.text(Target::Member(member.seat), "dj_active", self.patterns());
        Ok(a)
    }
    pub(super) fn tick(&mut self) -> Result<Actions, Error> {
        let mut a = Actions::default();
        self.fresh.tick();
        if self.club.controller.is_none() {
            return Ok(a);
        }
        let time = self.time;
        self.time -= 1;
        if time != 0 {
            return Ok(a);
        }
        self.time = 150;
        let mut new_correct = false;
        for index in 0..4 {
            if self.dirty[index] {
                let pattern = self.patterns[index];
                let value =
                    i32::from(pattern[0]) * 16 + i32::from(pattern[1]) * 4 + i32::from(pattern[2]);
                let distance = i32::from(self.club.dj[index]) - value;
                if distance == 0 && !self.correct[index] {
                    self.correct[index] = true;
                    new_correct = true;
                }
                self.fresh.send(value, index);
                let fraction = 1.0 - (distance as f32 / 64.0).abs();
                self.ratings[index] = fraction * fraction;
                self.dirty[index] = false;
            }
        }
        let average = self
            .ratings
            .iter()
            .map(|value| f64::from(*value))
            .sum::<f64>() as f32
            / 4.0;
        self.rating = ((100.0f32
            * (0.8f32 * average * self.fresh.good() + 0.25f32 * self.club.percentage()))
            as f64)
            .round_ties_even()
            .min(100.0) as i16;
        a.object(Target::Controller, 1, vec![self.rating]);
        if new_correct {
            a.peer(
                NC_FLOOR,
                signal(
                    GOOD_PATTERN,
                    vec![i64::from(self.group), i64::from(self.x), i64::from(self.y)],
                    Vec::new(),
                ),
            );
        } else if self.last_rating != self.rating && self.last_rating != -1 {
            let (kind, direction) = if self.last_rating > self.rating {
                (3, 0.0f32)
            } else {
                (
                    1,
                    (f64::from(self.rng.below(8)?) * std::f64::consts::PI / 4.0) as f32,
                )
            };
            a.peer(
                NC_FLOOR,
                signal(
                    PARTICLE,
                    vec![
                        i64::from(self.group),
                        kind,
                        i64::from(direction.to_bits()),
                        0,
                    ],
                    Vec::new(),
                ),
            );
        }
        self.last_rating = self.rating;
        Ok(a)
    }
    pub(super) fn peer(&mut self, source: &PeerSource, signal: &Signal) -> Result<Actions, Error> {
        if source.plugin != NIGHTCLUB {
            return Ok(Actions::default());
        }
        if self
            .club
            .controller
            .is_some_and(|controller| controller != source.group)
        {
            return Err(Error::InvalidPluginInput);
        }
        match signal.code {
            VIEW => {
                self.club.update(source, signal)?;
                Ok(Actions::default())
            }
            RESET => {
                self.club.controller = Some(source.group);
                self.reset()
            }
            DETACH => {
                self.club.controller = None;
                self.club.active = false;
                self.club.ticks = -1;
                Ok(Actions::default())
            }
            _ => Ok(Actions::default()),
        }
    }
    pub(super) fn validate(&self, roster: &Roster) -> bool {
        valid_single(roster)
            && self.group <= 3
            && self.fresh.valid(4)
            && self.club.valid()
            && self.patterns.iter().flatten().all(|digit| *digit < 4)
            && self
                .ratings
                .iter()
                .all(|r| r.is_finite() && (0.0..=1.0).contains(r))
            && (-1..=100).contains(&self.last_rating)
            && (-1..=100).contains(&self.rating)
            && (0..=150).contains(&self.time)
    }
    pub(super) fn validate_peers(&self, cluster: u64, peers: &[PeerGroup]) -> bool {
        self.club.validate_peers(cluster, peers)
    }
    pub(super) fn save(&self, w: &mut Writer) {
        self.rng.save(w);
        w.u8(self.group);
        w.i16(self.x);
        w.i16(self.y);
        self.fresh.save(w);
        for pattern in self.patterns {
            w.fixed(&pattern);
        }
        for value in self.dirty {
            w.bool(value);
        }
        for value in self.correct {
            w.bool(value);
        }
        for value in self.ratings {
            w.u32(value.to_bits());
        }
        w.i16(self.last_rating);
        w.i16(self.rating);
        w.i16(self.time);
        self.club.save(w);
    }
    pub(super) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let rng = Rng::restore(r)?;
        let group = r.u8()?;
        let x = r.i16()?;
        let y = r.i16()?;
        let fresh = Freshness::restore(r, 4)?;
        let mut patterns = [[0; 3]; 4];
        for pattern in &mut patterns {
            *pattern = r
                .take(3)?
                .try_into()
                .map_err(|_| Error::InvalidCheckpoint)?;
        }
        let mut dirty = [false; 4];
        for value in &mut dirty {
            *value = r.bool()?;
        }
        let mut correct = [false; 4];
        for value in &mut correct {
            *value = r.bool()?;
        }
        let mut ratings = [0.0; 4];
        for value in &mut ratings {
            *value = f32::from_bits(r.u32()?);
        }
        Ok(Self {
            rng,
            group,
            x,
            y,
            fresh,
            patterns,
            dirty,
            correct,
            ratings,
            last_rating: r.i16()?,
            rating: r.i16()?,
            time: r.i16()?,
            club: View::restore(r)?,
        })
    }
}

#[derive(Clone, Copy, Default)]
struct Observed {
    uid: Option<u16>,
    dance: u8,
}
#[derive(Clone)]
pub(super) struct Platform {
    group: u8,
    fresh: Freshness,
    section_ratings: Vec<f32>,
    correct: [bool; 3],
    rating: i16,
    active: Observed,
    observed: Observed,
    since_rating: u16,
    club: View,
}
impl Platform {
    pub(super) fn required_peers(&self) -> Vec<InstanceId> {
        self.club.controller.into_iter().collect()
    }

    pub(super) fn new(group: u8) -> Result<Self, Error> {
        if group > 3 {
            return Err(Error::InvalidPluginInput);
        }
        Ok(Self {
            group,
            fresh: Freshness::new(1),
            section_ratings: Vec::new(),
            correct: [false; 3],
            rating: 0,
            active: Observed::default(),
            observed: Observed::default(),
            since_rating: 0,
            club: View::new(),
        })
    }
    pub(super) fn start(&self) -> Actions {
        let mut a = Actions::default();
        a.peer(NIGHTCLUB, signal(REQUEST, Vec::new(), Vec::new()));
        a
    }
    pub(super) fn view(&self, member: Member) -> Actions {
        let mut a = Actions::default();
        a.text(
            Target::Member(member.seat),
            "dance_show",
            self.group.to_string(),
        );
        a
    }
    fn init_section(&mut self) -> Result<(), Error> {
        if usize::from(self.club.section) >= MAX_SECTIONS {
            return Err(Error::CounterExhausted);
        }
        while self.section_ratings.len() <= usize::from(self.club.section) {
            self.correct = [false; 3];
            self.section_ratings.push(0.0);
        }
        Ok(())
    }
    fn current_rating(&mut self, a: &mut Actions) -> Result<(), Error> {
        self.init_section()?;
        let mut total = 0;
        for (correct, weight) in self.correct.into_iter().zip([55, 25, 20]) {
            total += if correct { weight } else { 0 };
        }
        let total = (f64::from(total as f32 / 100.0).sqrt() * 100.0).round_ties_even() as i32;
        self.section_ratings[usize::from(self.club.section)] =
            25.0 * self.fresh.freshness() + total as f32 * 0.80 * self.fresh.good();
        let average = (self
            .section_ratings
            .iter()
            .map(|v| f64::from(*v))
            .sum::<f64>()
            / self.section_ratings.len() as f64) as f32;
        self.rating = f64::from(average + self.club.percentage() * 20.0)
            .round_ties_even()
            .min(100.0) as i16;
        self.since_rating = 0;
        a.object(Target::Controller, 1, vec![self.rating]);
        Ok(())
    }
    fn completed(&mut self, dance: u8, a: &mut Actions) -> Result<(), Error> {
        self.fresh.send(i32::from(dance), 0);
        self.init_section()?;
        let mut found = false;
        for index in 0..3 {
            if self.club.dances[index] == dance && !self.correct[index] {
                self.correct[index] = true;
                found = true;
            }
        }
        if found {
            a.peer(
                NC_FLOOR,
                signal(PARTICLE, vec![i64::from(self.group), 0, 0, 0], Vec::new()),
            );
            if self.correct.iter().all(|value| *value) {
                for frame in [-2, -4] {
                    a.peer(
                        NC_FLOOR,
                        signal(
                            PARTICLE,
                            vec![i64::from(self.group), 0, 0, frame],
                            Vec::new(),
                        ),
                    );
                }
            }
        }
        self.current_rating(a)
    }
    pub(super) fn tick(&mut self, roster: &Roster) -> Result<Actions, Error> {
        self.fresh.tick();
        let mut a = Actions::default();
        if first_member(roster).is_some() && self.club.active {
            if self.observed.uid != self.active.uid {
                if self.active.uid.is_some() {
                    self.completed(self.active.dance, &mut a)?;
                }
                self.active = self.observed;
            }
            self.since_rating += 1;
            if self.since_rating > 150 {
                self.current_rating(&mut a)?;
            }
        }
        Ok(a)
    }
    pub(super) fn message(
        &mut self,
        member: Member,
        event: &str,
        payload: &[u8],
    ) -> Result<Actions, Error> {
        let mut a = Actions::default();
        if event == "close" {
            a.close(Target::Member(member.seat));
            return Ok(a);
        }
        if event != "press_button" {
            return Err(Error::EventNotAllowed);
        }
        if let Ok(text) = std::str::from_utf8(payload)
            && let Ok(value) = text.trim().parse::<u8>()
        {
            a.object(
                Target::Controller,
                i16::from(value),
                vec![member.avatar_object],
            );
        }
        Ok(a)
    }
    pub(super) fn vm_event(&mut self, input: &VmInput, roster: &Roster) -> Result<Actions, Error> {
        let VmInput::DanceQueueObservation {
            avatar_id,
            active_uid,
            interaction,
            on_this_platform,
            queue_active,
        } = input
        else {
            return Err(Error::InvalidPluginInput);
        };
        let member = first_member(roster).ok_or(Error::PluginNotReady)?;
        if member.avatar_id != *avatar_id {
            return Err(Error::NotAuthorized);
        }
        self.observed = if *queue_active && *on_this_platform && *interaction > 5 {
            if *interaction > 29 || active_uid.is_none() {
                return Err(Error::InvalidPluginInput);
            }
            Observed {
                uid: *active_uid,
                dance: (*interaction - 6) as u8,
            }
        } else {
            Observed::default()
        };
        Ok(Actions::default())
    }
    pub(super) fn peer(&mut self, source: &PeerSource, signal: &Signal) -> Result<Actions, Error> {
        if source.plugin != NIGHTCLUB {
            return Ok(Actions::default());
        }
        if self
            .club
            .controller
            .is_some_and(|controller| controller != source.group)
        {
            return Err(Error::InvalidPluginInput);
        }
        match signal.code {
            VIEW => self.club.update(source, signal)?,
            RESET => {
                self.club.controller = Some(source.group);
                self.section_ratings.clear();
                self.rating = 25;
                self.correct = [false; 3];
                self.fresh = Freshness::new(1);
            }
            DETACH => {
                self.club.controller = None;
                self.club.active = false;
                self.club.ticks = -1;
            }
            _ => {}
        }
        Ok(Actions::default())
    }
    pub(super) fn leave(&mut self) -> Actions {
        self.active = Observed::default();
        self.observed = Observed::default();
        let mut a = Actions::default();
        a.close(Target::Controller);
        a
    }
    pub(super) fn validate(&self, roster: &Roster) -> bool {
        valid_single(roster)
            && self.group <= 3
            && self.fresh.valid(1)
            && self.club.valid()
            && self.section_ratings.len() <= MAX_SECTIONS
            && self
                .section_ratings
                .iter()
                .all(|value| value.is_finite() && (0.0..=105.0).contains(value))
            && (0..=100).contains(&self.rating)
            && self.active.dance < 24
            && self.observed.dance < 24
            && self.since_rating <= 150
    }
    pub(super) fn validate_peers(&self, cluster: u64, peers: &[PeerGroup]) -> bool {
        self.club.validate_peers(cluster, peers)
    }
    pub(super) fn save(&self, w: &mut Writer) {
        w.u8(self.group);
        self.fresh.save(w);
        w.u8(self.section_ratings.len() as u8);
        for rating in &self.section_ratings {
            w.u32(rating.to_bits());
        }
        for correct in self.correct {
            w.bool(correct);
        }
        w.i16(self.rating);
        for observed in [self.active, self.observed] {
            w.bool(observed.uid.is_some());
            if let Some(uid) = observed.uid {
                w.u16(uid);
            }
            w.u8(observed.dance);
        }
        w.u16(self.since_rating);
        self.club.save(w);
    }
    pub(super) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let group = r.u8()?;
        let fresh = Freshness::restore(r, 1)?;
        let count = r.u8()?;
        if usize::from(count) > MAX_SECTIONS {
            return Err(Error::InvalidCheckpoint);
        }
        let mut section_ratings = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            section_ratings.push(f32::from_bits(r.u32()?));
        }
        let mut correct = [false; 3];
        for value in &mut correct {
            *value = r.bool()?;
        }
        let rating = r.i16()?;
        let mut observed = [Observed::default(); 2];
        for value in &mut observed {
            value.uid = if r.bool()? { Some(r.u16()?) } else { None };
            value.dance = r.u8()?;
        }
        Ok(Self {
            group,
            fresh,
            section_ratings,
            correct,
            rating,
            active: observed[0],
            observed: observed[1],
            since_rating: r.u16()?,
            club: View::restore(r)?,
        })
    }
}

fn valid_objects(values: &[i16], maximum: usize) -> bool {
    values.len() <= maximum
        && values
            .iter()
            .enumerate()
            .all(|(i, value)| *value > 0 && !values[..i].contains(value))
}
fn save_objects(values: &[i16], w: &mut Writer) {
    w.u16(values.len() as u16);
    for value in values {
        w.i16(*value);
    }
}
fn restore_objects(r: &mut Reader<'_>, max: usize) -> Result<Vec<i16>, Error> {
    let count = usize::from(r.u16()?);
    if count > max {
        return Err(Error::InvalidCheckpoint);
    }
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(r.i16()?);
    }
    if !valid_objects(&values, max) {
        return Err(Error::InvalidCheckpoint);
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_freshness_miss_does_not_add_command_to_history() {
        let mut f = Freshness::new(1);
        f.send(2, 0);
        assert_eq!(f.history[0], [-1, -1, -1]);
        assert_eq!(f.amount.to_bits(), 0.15f32.to_bits());
        f.send(2, 0);
        assert_eq!(f.amount.to_bits(), 0.30f32.to_bits());
        for _ in 0..30 {
            f.tick();
        }
        // Actual unchanged C# output is c9 8f 94 3e (little endian).
        assert_eq!(f.amount.to_bits(), 0x3e948fc9);
    }
}
