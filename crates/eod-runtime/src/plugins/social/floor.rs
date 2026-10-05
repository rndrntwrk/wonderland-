// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
use super::{club::*, util::*, *};

const TEXT: [&str; 12] = [
    "FreeSO", "PARTY", "DANCE", "~(o.o)~", ";)", "WOW", "\\(o.o)/", "<3", "BOO", "LAME", ":(",
    "YOU SUCK",
];
const DIAMOND: [&str; 9] = [
    "....#....",
    "...###...",
    "..#####..",
    ".#######.",
    "#########",
    ".#######.",
    "..#####..",
    "...###...",
    "....#....",
];
const HEART: [&str; 9] = [
    ".........",
    ".###.###.",
    "#########",
    "#########",
    "#########",
    ".#######.",
    "..#####..",
    "...###...",
    "....#....",
];
const STAR: [&str; 9] = [
    ".........",
    "....#....",
    "...###...",
    "#########",
    ".#######.",
    ".#######.",
    "###...###",
    "#.......#",
    ".........",
];

#[derive(Clone)]
struct Particle {
    kind: u8,
    group: u8,
    direction: f32,
    frame: i32,
}
#[derive(Clone)]
pub(super) struct Floor {
    rng: Rng,
    tiles: Vec<FloorTile>,
    width: i32,
    height: i32,
    center: (i32, i32),
    diagonal: i32,
    objects: Vec<i16>,
    pixels: Vec<u8>,
    discovered: bool,
    animation: u8,
    direction: i16,
    color: u8,
    random_animation: u8,
    text: Option<u8>,
    temporary: u8,
    ratings: [i16; 4],
    memory: [i32; 6],
    particles: Vec<Particle>,
    frame: i32,
    tock: u8,
    controller: Option<InstanceId>,
}
impl Floor {
    pub(super) fn required_peers(&self) -> Vec<InstanceId> {
        self.controller.into_iter().collect()
    }

    pub(super) fn new(seed: Seed, tiles: Vec<FloorTile>) -> Result<Self, Error> {
        let mut value = Self {
            rng: Rng::new(seed.0),
            tiles: Vec::new(),
            width: 0,
            height: 0,
            center: (0, 0),
            diagonal: 0,
            objects: Vec::new(),
            pixels: Vec::new(),
            discovered: false,
            animation: 0,
            direction: 0,
            color: 0,
            random_animation: 0,
            text: None,
            temporary: 0,
            ratings: [0; 4],
            memory: [0; 6],
            particles: Vec::new(),
            frame: 0,
            tock: 0,
            controller: None,
        };
        value.discover(tiles)?;
        value.discovered = false;
        Ok(value)
    }
    fn discover(&mut self, tiles: Vec<FloorTile>) -> Result<(), Error> {
        if tiles.len() > 4096
            || tiles.iter().enumerate().any(|(i, tile)| {
                tile.object <= 0
                    || tiles[..i].iter().any(|other| {
                        other.object == tile.object || other.x == tile.x && other.y == tile.y
                    })
            })
        {
            return Err(Error::InvalidPluginInput);
        }
        if tiles.is_empty() {
            self.tiles = tiles;
            self.width = 0;
            self.height = 0;
            self.diagonal = 0;
            self.center = (0, 0);
            self.objects.clear();
            self.pixels.clear();
            self.discovered = false;
            self.particles.clear();
            return Ok(());
        }
        let min_x = i32::from(
            tiles
                .iter()
                .map(|tile| tile.x)
                .min()
                .ok_or(Error::InvalidPluginInput)?,
        );
        let max_x = i32::from(
            tiles
                .iter()
                .map(|tile| tile.x)
                .max()
                .ok_or(Error::InvalidPluginInput)?,
        );
        let min_y = i32::from(
            tiles
                .iter()
                .map(|tile| tile.y)
                .min()
                .ok_or(Error::InvalidPluginInput)?,
        );
        let max_y = i32::from(
            tiles
                .iter()
                .map(|tile| tile.y)
                .max()
                .ok_or(Error::InvalidPluginInput)?,
        );
        let width = max_x - min_x + 1;
        let height = max_y - min_y + 1;
        if width > 64 || height > 64 {
            return Err(Error::InvalidPluginInput);
        }
        self.width = width;
        self.height = height;
        self.diagonal = f64::from(width * width + height * height).sqrt().ceil() as i32;
        self.center = ((min_x + max_x) / 2, (min_y + max_y) / 2);
        self.objects = vec![0; (width * height) as usize];
        self.pixels = vec![0; self.objects.len()];
        for tile in &tiles {
            let index = i32::from(tile.x) - min_x + (i32::from(tile.y) - min_y) * width;
            self.objects[index as usize] = tile.object;
        }
        self.tiles = tiles;
        self.discovered = true;
        self.particles.clear();
        Ok(())
    }
    pub(super) fn start(&self) -> Actions {
        let mut a = Actions::default();
        a.peer(NIGHTCLUB, signal(FLOOR_READY, Vec::new(), Vec::new()));
        a
    }
    fn pixel(&mut self, x: i32, y: i32, color: u8) {
        if x >= 0 && x < self.width && y >= 0 && y < self.height {
            self.pixels[(x + y * self.width) as usize] = color;
        }
    }
    fn rect(&mut self, x: i32, y: i32, width: i32, height: i32, color: u8) {
        if width <= 0 || height <= 0 {
            return;
        }
        for y in y.max(0)..(y + height).min(self.height) {
            for x in x.max(0)..(x + width).min(self.width) {
                self.pixel(x, y, color);
            }
        }
    }
    fn image(&mut self, x: i32, y: i32, image: &[&str; 9], color: u8) {
        for (dy, row) in image.iter().enumerate() {
            for (dx, pixel) in row.bytes().enumerate() {
                if pixel == b'#' {
                    self.pixel(x + dx as i32, y + dy as i32, color);
                }
            }
        }
    }
    fn text(&mut self, value: &str, x: i32, y: i32, color: u8) {
        for (index, character) in value.bytes().enumerate() {
            let x = x + index as i32 * 4;
            if x > -3 && x < self.width {
                for row in 0..6 {
                    let line = super::font::line(character, row);
                    for column in 0..3 {
                        if line >> (3 - column) & 1 != 0 {
                            self.pixel(x + column, y + row, color);
                        }
                    }
                }
            }
        }
    }
    fn line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u8) {
        if (y2 - y1).abs() < (x2 - x1).abs() {
            let (x1, y1, x2, y2) = if x1 > x2 {
                (x2, y2, x1, y1)
            } else {
                (x1, y1, x2, y2)
            };
            let dx = x2 - x1;
            let dy = (y2 - y1).abs();
            let step = if y2 - y1 < 0 { -1 } else { 1 };
            let mut error = 2 * dy - dx;
            let mut y = y1;
            for x in x1..=x2 {
                self.pixel(x, y, color);
                if error > 0 {
                    y += step;
                    error -= 2 * dx;
                }
                error += 2 * dy;
            }
        } else {
            let (x1, y1, x2, y2) = if y1 > y2 {
                (x2, y2, x1, y1)
            } else {
                (x1, y1, x2, y2)
            };
            let dx = (x2 - x1).abs();
            let dy = y2 - y1;
            let step = if x2 - x1 < 0 { -1 } else { 1 };
            let mut error = 2 * dx - dy;
            let mut x = x1;
            for y in y1..=y2 {
                self.pixel(x, y, color);
                if error > 0 {
                    x += step;
                    error -= 2 * dy;
                }
                error += 2 * dx;
            }
        }
    }
    fn vector_line(&mut self, first: (f32, f32), second: (f32, f32), color: u8, outline: bool) {
        let x1 = first.0.round_ties_even() as i32;
        let y1 = first.1.round_ties_even() as i32;
        let x2 = second.0.round_ties_even() as i32;
        let y2 = second.1.round_ties_even() as i32;
        if outline {
            for (x, y) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
                self.line(x1 + x, y1 + y, x2 + x, y2 + y, color);
            }
        } else {
            self.line(x1, y1, x2, y2, color);
        }
    }
    fn random_color(&mut self) -> Result<u8, Error> {
        let mut distance = self.rng.below(
            self.ratings
                .iter()
                .map(|v| u32::from(*v as u16))
                .sum::<u32>()
                + 50,
        )? as i32;
        for (index, rating) in self.ratings.iter().enumerate() {
            if distance < i32::from(*rating) {
                return Ok(index as u8 * 2 + 2);
            }
            distance -= i32::from(*rating);
        }
        Ok(10)
    }
    fn random(&mut self, frame: i32, lit: u8, color: u8) -> Result<(), Error> {
        if frame % 60 == 0 {
            self.random_animation = self.rng.below(5)? as u8;
            if self.random_animation == 4 {
                let average = self
                    .ratings
                    .iter()
                    .map(|value| i32::from(*value))
                    .sum::<i32>() as f64
                    / 4.0;
                let bank = if average < 25.0 {
                    2
                } else if average > 75.0 {
                    1
                } else {
                    0
                };
                // Source adds bank, rather than multiplying it by four.
                self.text = Some((self.rng.below(4)? + bank) as u8);
            } else if self.random_animation == 1 {
                self.memory = [0; 6];
            } else {
                for item in &mut self.memory {
                    *item = self.rng.below(i32::MAX as u32)? as i32;
                }
            }
        }
        match self.random_animation {
            0 => {
                for y in 0..self.height {
                    for x in 0..self.width {
                        if self.rng.below(4)? == 0 {
                            let color = self.random_color()?;
                            self.pixel(x, y, color);
                        }
                    }
                }
            }
            1 => {
                for y in 0..self.height {
                    for x in 0..self.width {
                        let dx = x - self.width / 2;
                        let dy = y - self.height / 2;
                        let mut distance = f64::from(dx * dx + dy * dy).sqrt();
                        distance -= f64::from(frame as f32 / 3.0);
                        let color = -(((distance - f64::from(self.diagonal)) / 2.0) as i32 % 5);
                        self.pixel(x, y, (color * 2 + 1 + i32::from(lit)) as u8);
                    }
                }
            }
            2 => {
                let repeat = frame % self.diagonal;
                let first = if repeat <= self.diagonal / 2 { 3 } else { 0 };
                let second = 3 - first;
                let first_radius = if repeat <= self.diagonal / 2 {
                    frame.wrapping_add(self.diagonal / 2) % self.diagonal
                } else {
                    repeat
                };
                let second_radius = if repeat > self.diagonal / 2 {
                    frame.wrapping_add(self.diagonal / 2) % self.diagonal
                } else {
                    repeat
                };
                if repeat == 0 {
                    self.memory[0] = i32::from(self.random_color()?);
                    self.memory[1] = self.rng.below(self.width as u32)? as i32;
                    self.memory[2] = self.rng.below(self.height as u32)? as i32;
                }
                if repeat == self.diagonal / 2 {
                    self.memory[3] = i32::from(self.random_color()?);
                    self.memory[4] = self.rng.below(self.width as u32)? as i32;
                    self.memory[5] = self.rng.below(self.height as u32)? as i32;
                }
                for y in 0..self.height {
                    for x in 0..self.width {
                        for (offset, radius) in [(first, first_radius), (second, second_radius)] {
                            let dx = x.wrapping_sub(self.memory[offset + 1]);
                            let dy = y.wrapping_sub(self.memory[offset + 2]);
                            let squared = dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy));
                            if f64::from(squared).sqrt() <= f64::from(radius) {
                                self.pixel(x, y, (self.memory[offset] as u8).wrapping_add(lit));
                            }
                        }
                    }
                }
            }
            3 => {
                let fall_distance = self.height + 4;
                for x in 0..self.width {
                    // A tiny or malformed source floor can make fallFreq zero.
                    // Bound that divisor so a native frame cannot panic.
                    let frequency =
                        (self.height + self.memory[x as usize % 6].wrapping_add(x) % 9 - 4).max(1);
                    let position =
                        ((frame % frequency) * fall_distance / frequency) % fall_distance;
                    let color = ((x % 5) * 2 + 1) as u8;
                    for y in 0..self.height {
                        if y <= position - 3 {
                            self.pixel(x, y, 0);
                        } else if y < position && y > position - 3 {
                            self.pixel(x, y, color + 1);
                        } else if y == position {
                            self.pixel(x, y, color);
                        }
                    }
                }
            }
            4 => {
                self.pixels.fill((10 + lit) % 11);
                let text = self.text.map_or("FreeSO", |index| TEXT[usize::from(index)]);
                let scroll = self.frame % (self.width + (text.len() as i32 + 1) * 4);
                self.text(text, self.width - scroll, (self.height - 1) / 2 - 2, color);
                let border = self.height / 2 - 3;
                self.rect(0, 0, self.width, border, color);
                self.rect(0, self.height - border, self.width, border, color);
            }
            _ => return Err(Error::InvalidPluginInput),
        }
        Ok(())
    }
    fn draw_particle(&mut self, particle: &Particle) -> bool {
        let frame = particle.frame;
        let color = particle.group * 2 + 1;
        let w = self.width;
        let h = self.height;
        let d = self.diagonal;
        let rotate = |(x, y): (f32, f32)| {
            let sine = (-particle.direction).sin();
            let cosine = (-particle.direction).cos();
            (
                x * cosine - y * sine + w as f32 / 2.0 - 0.25,
                x * sine + y * cosine + h as f32 / 2.0 - 0.25,
            )
        };
        match particle.kind {
            0 => {
                let width = w - frame * 2;
                let height = h - frame * 2;
                self.rect(frame - 1, frame - 1, 3, height, 0);
                self.rect(frame + width - 2, frame - 1, 3, height, 0);
                self.rect(frame - 1, frame - 1, width, 3, 0);
                self.rect(frame - 1, frame + height - 2, width, 3, 0);
                self.rect(frame, frame, 1, height, color);
                self.rect(frame + width - 1, frame, 1, height, color);
                self.rect(frame, frame, width, 1, color);
                self.rect(frame, frame + height - 1, width, 1, color);
                frame < w / 2
            }
            1 => {
                let first = rotate((-d as f32, (d / 2 - frame) as f32));
                let second = rotate((d as f32, (d / 2 - frame) as f32));
                self.vector_line(first, second, 0, true);
                self.vector_line(first, second, color, false);
                frame < d
            }
            2 => {
                let frame = frame as f32;
                let points = [
                    (0.0, 16.0 - frame),
                    (0.0, 8.0 - frame),
                    (5.0, 13.0 - frame),
                    (-5.0, 13.0 - frame),
                    (0.0, 9.25 - frame),
                    (5.0, 14.25 - frame),
                    (-5.0, 14.25 - frame),
                ]
                .map(rotate);
                for (first, second) in [(0, 1), (1, 2), (1, 3), (4, 5), (4, 6)] {
                    self.vector_line(points[first], points[second], 0, true);
                }
                for (first, second) in [(0, 1), (1, 2), (1, 3), (4, 5), (4, 6)] {
                    self.vector_line(points[first], points[second], color, false);
                }
                particle.frame < d + 8
            }
            3 => {
                let frame_mod = frame % (w / 2).max(1);
                let y = if particle.group.is_multiple_of(2) {
                    0
                } else {
                    h - 2
                };
                let left = w / 2 - frame_mod;
                let right = w / 2 + frame_mod;
                self.rect(0, y, w, 2, 0);
                self.rect(left - 1, y, 3, 2, color + 1);
                self.rect(right - 1, y, 3, 2, color + 1);
                self.rect(left, y, 1, 2, color);
                self.rect(right, y, 1, 2, color);
                frame < w * 2
            }
            _ => false,
        }
    }
    fn draw(&mut self) -> Result<Actions, Error> {
        let frame = self.frame;
        self.frame = self
            .frame
            .checked_add(1)
            .filter(|value| *value != i32::MAX)
            .ok_or(Error::CounterExhausted)?;
        let lit = ((frame / 10) % 2) as u8;
        let color = self.color.wrapping_add(lit);
        match self.animation {
            0 => self.pixels.fill(0),
            1 => self.pixels.fill(color),
            5..=7 => {
                self.pixels.fill(10);
                let image = match self.animation {
                    5 => &HEART,
                    6 => &DIAMOND,
                    _ => &STAR,
                };
                self.image(self.width / 2 - 4, self.height / 2 - 4, image, color);
                let timer = self.temporary;
                self.temporary += 1;
                if timer > 75 {
                    self.animation = 2;
                }
            }
            _ => self.random(frame, lit, color)?,
        }
        let particles = std::mem::take(&mut self.particles);
        for mut particle in particles {
            if self.draw_particle(&particle) {
                particle.frame += 1;
                self.particles.push(particle);
            }
        }
        let mut objects = Vec::with_capacity(self.tiles.len());
        let mut graphics = Vec::with_capacity(self.tiles.len());
        // Original VMNetBatchGraphicCmd ignores object ID zero. Omit the empty
        // cells from the native command while retaining their raster positions.
        for (object, graphic) in self.objects.iter().zip(&self.pixels) {
            if *object != 0 {
                objects.push(*object);
                graphics.push(*graphic);
            }
        }
        let mut a = Actions::default();
        if !objects.is_empty() {
            a.command(NativeCommand::BatchGraphics { objects, graphics });
        }
        Ok(a)
    }
    pub(super) fn tick(&mut self) -> Result<Actions, Error> {
        let mut a = Actions::default();
        if self.tock == 0 && !self.tiles.is_empty() {
            if self.discovered {
                a = self.draw()?;
            } else {
                self.discovered = true;
            }
        }
        self.tock = (self.tock + 1) % 3;
        Ok(a)
    }
    pub(super) fn vm_event(&mut self, input: &VmInput) -> Result<Actions, Error> {
        match input {
            VmInput::FloorDiscover { tiles } => self.discover(tiles.clone())?,
            VmInput::FloorAnimation {
                animation,
                color,
                direction,
            } => {
                if *animation > 7 {
                    return Err(Error::InvalidPluginInput);
                }
                self.color = *color as u8;
                if ![3, 4].contains(animation) {
                    self.animation = *animation;
                    if *animation > 2 {
                        self.temporary = 0;
                    }
                    self.direction = *direction;
                }
            }
            VmInput::FloorRatings { ratings } => {
                if ratings.iter().any(|rating| !(0..=100).contains(rating)) {
                    return Err(Error::InvalidPluginInput);
                }
                self.ratings = *ratings;
            }
            _ => return Err(Error::InvalidPluginInput),
        }
        Ok(Actions::default())
    }
    pub(super) fn peer(&mut self, source: &PeerSource, signal: &Signal) -> Result<Actions, Error> {
        if source.plugin == NIGHTCLUB {
            if self
                .controller
                .is_some_and(|controller| controller != source.group)
            {
                return Err(Error::InvalidPluginInput);
            }
            if signal.code == REQUEST {
                self.controller = Some(source.group);
                return Ok(self.start());
            }
            if signal.code == DETACH {
                self.controller = None;
            }
            return Ok(Actions::default());
        }
        if ![DJ_STATION, DANCE_PLATFORM].contains(&source.plugin)
            || ![PARTICLE, GOOD_PATTERN].contains(&signal.code)
        {
            return Ok(Actions::default());
        }
        if self.particles.len() >= 256 {
            return Err(Error::QueueFull);
        }
        let particle = if signal.code == GOOD_PATTERN && source.plugin == DJ_STATION {
            if signal.numbers.len() != 3 {
                return Err(Error::InvalidPluginInput);
            }
            let group = u8::try_from(signal.numbers[0]).map_err(|_| Error::InvalidPluginInput)?;
            let x = i16::try_from(signal.numbers[1]).map_err(|_| Error::InvalidPluginInput)?;
            let y = i16::try_from(signal.numbers[2]).map_err(|_| Error::InvalidPluginInput)?;
            let direction = f64::from(self.center.0 - i32::from(x))
                .atan2(f64::from(self.center.1 - i32::from(y))) as f32;
            Particle {
                kind: 2,
                group,
                direction,
                frame: 0,
            }
        } else {
            if signal.numbers.len() != 4 {
                return Err(Error::InvalidPluginInput);
            }
            Particle {
                group: u8::try_from(signal.numbers[0]).map_err(|_| Error::InvalidPluginInput)?,
                kind: u8::try_from(signal.numbers[1]).map_err(|_| Error::InvalidPluginInput)?,
                direction: f32::from_bits(
                    u32::try_from(signal.numbers[2]).map_err(|_| Error::InvalidPluginInput)?,
                ),
                frame: i32::try_from(signal.numbers[3]).map_err(|_| Error::InvalidPluginInput)?,
            }
        };
        if !Self::valid_particle(&particle) {
            return Err(Error::InvalidPluginInput);
        }
        if !self.tiles.is_empty() {
            self.particles.push(particle);
        }
        Ok(Actions::default())
    }
    pub(super) fn shutdown(&self) -> Actions {
        let mut a = Actions::default();
        a.peer(NIGHTCLUB, signal(DETACH, Vec::new(), Vec::new()));
        a
    }
    fn valid_particle(p: &Particle) -> bool {
        p.kind <= 3
            && p.group <= 3
            && p.direction.is_finite()
            && p.direction.abs() <= 8.0
            && (-4..=128).contains(&p.frame)
            && (p.frame >= 0 || p.kind == 0)
    }
    pub(super) fn validate(&self, roster: &Roster) -> bool {
        if !valid_single(roster)
            || self.animation > 7
            || [3, 4].contains(&self.animation)
            || self.random_animation > 4
            || self.text.is_some_and(|index| index >= 12)
            || self.temporary > 77
            || self
                .ratings
                .iter()
                .any(|rating| !(0..=100).contains(rating))
            || self.particles.len() > 256
            || self.particles.iter().any(|p| !Self::valid_particle(p))
            || !(0..i32::MAX).contains(&self.frame)
            || self.tock > 2
        {
            return false;
        }
        if self.width < 0
            || self.height < 0
            || self.width > 64
            || self.height > 64
            || self.objects.len() != (self.width * self.height) as usize
            || self.pixels.len() != self.objects.len()
        {
            return false;
        }
        let mut reconstructed = match Self::new(Seed::new(1), self.tiles.clone()) {
            Ok(value) => value,
            Err(_) => return false,
        };
        reconstructed.discovered = self.discovered;
        self.width == reconstructed.width
            && self.height == reconstructed.height
            && self.center == reconstructed.center
            && self.diagonal == reconstructed.diagonal
            && self.objects == reconstructed.objects
            && (!self.discovered || !self.tiles.is_empty())
    }
    pub(super) fn validate_peers(&self, cluster: u64, peers: &[PeerGroup]) -> bool {
        self.controller.is_none_or(|group| {
            linked_peer(peers, group, cluster, NIGHTCLUB)
                .is_some_and(|peer| peer.roster.iter().all(Option::is_none))
        })
    }
    pub(super) fn save(&self, w: &mut Writer) {
        self.rng.save(w);
        w.u16(self.tiles.len() as u16);
        for tile in &self.tiles {
            w.i16(tile.object);
            w.i16(tile.x);
            w.i16(tile.y);
        }
        w.i32(self.width);
        w.i32(self.height);
        w.i32(self.center.0);
        w.i32(self.center.1);
        w.i32(self.diagonal);
        w.u16(self.objects.len() as u16);
        for object in &self.objects {
            w.i16(*object);
        }
        w.bytes(&self.pixels);
        w.bool(self.discovered);
        w.u8(self.animation);
        w.i16(self.direction);
        w.u8(self.color);
        w.u8(self.random_animation);
        w.u8(self.text.unwrap_or(255));
        w.u8(self.temporary);
        for rating in self.ratings {
            w.i16(rating);
        }
        for value in self.memory {
            w.i32(value);
        }
        w.u16(self.particles.len() as u16);
        for p in &self.particles {
            w.u8(p.kind);
            w.u8(p.group);
            w.u32(p.direction.to_bits());
            w.i32(p.frame);
        }
        w.i32(self.frame);
        w.u8(self.tock);
        save_option_id(self.controller, w);
    }
    pub(super) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let rng = Rng::restore(r)?;
        let count = usize::from(r.u16()?);
        if count > 4096 {
            return Err(Error::InvalidCheckpoint);
        }
        let mut tiles = Vec::with_capacity(count);
        for _ in 0..count {
            tiles.push(FloorTile {
                object: r.i16()?,
                x: r.i16()?,
                y: r.i16()?,
            });
        }
        let width = r.i32()?;
        let height = r.i32()?;
        let center = (r.i32()?, r.i32()?);
        let diagonal = r.i32()?;
        let count = usize::from(r.u16()?);
        if count > 4096 {
            return Err(Error::InvalidCheckpoint);
        }
        let mut objects = Vec::with_capacity(count);
        for _ in 0..count {
            objects.push(r.i16()?);
        }
        let pixels = r.bytes(4096)?;
        let discovered = r.bool()?;
        let animation = r.u8()?;
        let direction = r.i16()?;
        let color = r.u8()?;
        let random_animation = r.u8()?;
        let text = match r.u8()? {
            255 => None,
            value => Some(value),
        };
        let temporary = r.u8()?;
        let mut ratings = [0; 4];
        for value in &mut ratings {
            *value = r.i16()?;
        }
        let mut memory = [0; 6];
        for value in &mut memory {
            *value = r.i32()?;
        }
        let count = usize::from(r.u16()?);
        if count > 256 {
            return Err(Error::InvalidCheckpoint);
        }
        let mut particles = Vec::with_capacity(count);
        for _ in 0..count {
            particles.push(Particle {
                kind: r.u8()?,
                group: r.u8()?,
                direction: f32::from_bits(r.u32()?),
                frame: r.i32()?,
            });
        }
        Ok(Self {
            rng,
            tiles,
            width,
            height,
            center,
            diagonal,
            objects,
            pixels,
            discovered,
            animation,
            direction,
            color,
            random_animation,
            text,
            temporary,
            ratings,
            memory,
            particles,
            frame: r.i32()?,
            tock: r.u8()?,
            controller: restore_option_id(r)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn grid(width: i16, height: i16) -> Floor {
        Floor::new(
            Seed::new(1),
            (0..height)
                .flat_map(|y| {
                    (0..width).map(move |x| FloorTile {
                        object: 1 + x + y * width,
                        x,
                        y,
                    })
                })
                .collect(),
        )
        .unwrap()
    }
    fn hex(value: &[u8]) -> String {
        value.iter().map(|byte| format!("{byte:02x}")).collect()
    }
    #[test]
    fn rainbow_and_font_match_unchanged_csharp_oracle_literals() {
        let mut floor = grid(9, 9);
        floor.random_animation = 1;
        floor.random(1, 0, 3).unwrap();
        assert_eq!(
            hex(&floor.pixels),
            "070909090909090907090909010101090909090901010101010909090101010301010109090101030303010109090101010301010109090901010101010909090909010101090909070909090909090907"
        );
        let glyphs: Vec<_> = b"FSOpg~"
            .iter()
            .flat_map(|character| (0..6).map(move |row| font::line(*character, row)))
            .collect();
        assert_eq!(
            hex(&glyphs),
            "06080e080800060804020c00040a0a0a0400000c0a0a0c0800040a06020c040a00000000"
        );
    }
    #[test]
    fn restored_circle_animation_wraps_radius_near_frame_limit() {
        // Original unchecked int addition produces a negative radius for the
        // offset circle. Exercise both sides of the half-diagonal selection.
        for (frame, radius) in [(i32::MAX - 27, 9), (i32::MAX - 44, 83)] {
            let mut floor = grid(64, 64);
            floor.frame = frame;
            floor.animation = 2;
            floor.random_animation = 2;
            floor.discovered = true;
            floor.memory = [2, 0, 0, 6, 63, 63];
            floor.pixels.fill(10);
            assert_eq!(floor.diagonal, 91);
            assert!(floor.validate(&[None; 16]));

            let mut saved = Writer::default();
            floor.save(&mut saved);
            let mut reader = Reader::new(&saved.0);
            let mut restored = Floor::restore(&mut reader).unwrap();
            reader.finish().unwrap();
            assert!(restored.validate(&[None; 16]));
            let output = restored.tick().unwrap();

            let expected: Vec<_> = (0..64)
                .flat_map(|y| {
                    (0..64).map(move |x| {
                        if x * x + y * y <= radius * radius {
                            2
                        } else {
                            10
                        }
                    })
                })
                .collect();
            assert_eq!(restored.pixels, expected);
            assert_eq!((restored.frame, restored.tock), (frame + 1, 1));
            match output.items.as_slice() {
                [Action::Command(NativeCommand::BatchGraphics { objects, graphics })] => {
                    assert_eq!(objects.len(), 4096);
                    assert_eq!(graphics, &expected);
                }
                _ => panic!("expected one complete native floor update"),
            }
        }
    }
    #[test]
    fn tiny_floor_matrix_and_colder_have_defined_divisors() {
        let mut floor = grid(1, 1);
        floor.random_animation = 3;
        floor.memory = [3; 6]; // Original Matrix fallFreq would be zero.
        floor.random(1, 0, 3).unwrap();
        let particle = Particle {
            kind: 3,
            group: 0,
            direction: 0.0,
            frame: 0,
        };
        assert!(floor.draw_particle(&particle)); // Original Colder modulo width/2 is zero.
        assert_eq!(floor.pixels.len(), 1);
        assert!(floor.validate(&[None; 16]));
    }
}
