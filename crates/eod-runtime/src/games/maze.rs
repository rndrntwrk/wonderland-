// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Two-person maze and source recursive-backtracking/BFS order.
use super::{rng::NativeRng, *};

const ROWS: usize = 8;
const COLUMNS: usize = 36;
const CELLS: usize = ROWS * COLUMNS;
const OPPOSITE: [usize; 4] = [3, 2, 1, 0];
// Source MazeWallConfigCodes is an enum, not a bit mask. Index is open NWES bits.
const WALL_CODES: [u8; 16] = [15, 11, 7, 13, 6, 10, 4, 14, 5, 9, 3, 12, 2, 8, 1, 0];
const COLOR_EVENTS: [&str; 4] = [
    "TSOMaze_BlueIcon",
    "TSOMaze_GreenIcon",
    "TSOMaze_RedIcon",
    "TSOMaze_YellowIcon",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MazeRole {
    Logic,
    Charisma,
}
impl MazeRole {
    pub(crate) fn seat(self) -> usize {
        match self {
            Self::Logic => 0,
            Self::Charisma => 1,
        }
    }
}

#[derive(Clone, Copy)]
struct Cell {
    open: u8,
    color: u8,
}

#[derive(Clone)]
pub(crate) struct Maze {
    phase: u8,
    pending: Option<u8>,
    cooldown: u16,
    remaining: u16,
    tock: u8,
    logic_initialized: bool,
    charisma_initialized: bool,
    solved: bool,
    position: u16,
    origin_color: u8,
    target_moves: u8,
    exit: u16,
    cells: [Cell; CELLS],
    color_cells: [Vec<u16>; 4],
    solution: Vec<u16>,
    rng: NativeRng,
    maze_rng: NativeRng,
}

fn neighbor(cell: usize, direction: usize) -> Option<usize> {
    let row = cell / COLUMNS;
    let column = cell % COLUMNS;
    match direction {
        0 if row > 0 => Some(cell - COLUMNS),
        1 if column > 0 => Some(cell - 1),
        2 if column + 1 < COLUMNS => Some(cell + 1),
        3 if row + 1 < ROWS => Some(cell + COLUMNS),
        _ => None,
    }
}
fn path(cells: &[Cell; CELLS], origin: usize, target: u8) -> Vec<u16> {
    let mut parents = [usize::MAX; CELLS];
    let mut depths = [0u8; CELLS];
    let mut queue = [0usize; CELLS];
    let mut read = 0;
    let mut written = 1;
    let mut last = origin;
    parents[origin] = origin;
    queue[0] = origin;
    while read < written {
        let current = queue[read];
        read += 1;
        last = current;
        if depths[current] == target {
            break;
        }
        for direction in 0..4 {
            if cells[current].open & (1 << direction) == 0 {
                continue;
            }
            if let Some(next) = neighbor(current, direction)
                && parents[next] == usize::MAX
            {
                parents[next] = current;
                depths[next] = depths[current] + 1;
                queue[written] = next;
                written += 1;
            }
        }
    }
    let mut result = Vec::with_capacity(41);
    while last != origin {
        result.push(last as u16);
        last = parents[last];
    }
    result.push(origin as u16);
    result.reverse();
    result
}

impl Maze {
    pub(crate) fn new(seed: u64) -> Result<Self, Error> {
        let mut result = Self {
            phase: 0,
            pending: None,
            cooldown: 0,
            remaining: 0,
            tock: 0,
            logic_initialized: false,
            charisma_initialized: false,
            solved: false,
            position: 0,
            origin_color: 0,
            target_moves: 20,
            exit: 0,
            cells: [Cell { open: 0, color: 4 }; CELLS],
            color_cells: std::array::from_fn(|_| Vec::with_capacity(21)),
            solution: Vec::with_capacity(41),
            rng: NativeRng::new(seed),
            maze_rng: NativeRng::new(seed ^ 0x4d415a455f425549),
        };
        // Native MAZE-INIT-GUARD: realize the source's pending initial Waiting
        // transition before any join can overwrite it with Ready. This prevents
        // the original's uninitialized-map shutdown path for back-to-back joins.
        result.reset()?;
        Ok(result)
    }
    fn reset(&mut self) -> Result<(), Error> {
        self.solved = false;
        self.logic_initialized = false;
        self.charisma_initialized = false;
        self.cells = [Cell { open: 0, color: 4 }; CELLS];
        for cells in &mut self.color_cells {
            cells.clear();
        }
        let mut pools: Vec<(u8, u16)> = vec![
            (4, 28),
            (4, 28),
            (0, 14),
            (4, 28),
            (4, 28),
            (1, 14),
            (4, 28),
            (4, 28),
            (2, 15),
            (4, 28),
            (4, 28),
            (3, 15),
        ];
        let origin = [
            0,
            COLUMNS - 1,
            (ROWS - 1) * COLUMNS,
            CELLS - 1,
            (ROWS / 2) * COLUMNS + COLUMNS / 2,
        ][self.rng.below(5)?];
        let mut visited = [false; CELLS];
        let mut stack = Vec::with_capacity(CELLS);
        let mut current = origin;
        visited[current] = true;
        loop {
            let mut options = [(0usize, 0usize); 4];
            let mut count = 0;
            for direction in 0..4 {
                if let Some(next) = neighbor(current, direction)
                    && !visited[next]
                {
                    options[count] = (direction, next);
                    count += 1;
                }
            }
            if count != 0 {
                let (direction, next) = options[self.maze_rng.below(count)?];
                stack.push(current);
                self.cells[current].open |= 1 << direction;
                self.cells[next].open |= 1 << OPPOSITE[direction];
                visited[next] = true;
                current = next;
            } else {
                // OnFinalProcessingCell source callback order, not row order.
                let color = if pools.len() > 1 {
                    let index = self.rng.below(pools.len())?;
                    let color = pools[index].0;
                    pools[index].1 -= 1;
                    if pools[index].1 == 0 {
                        pools.remove(index);
                    }
                    color
                } else {
                    pools[0].0
                };
                self.cells[current].color = color;
                if color < 4 {
                    self.color_cells[usize::from(color)].push(current as u16);
                }
                if let Some(previous) = stack.pop() {
                    current = previous;
                } else {
                    break;
                }
            }
        }
        self.origin_color = self.rng.below(4)? as u8;
        let origins = &self.color_cells[usize::from(self.origin_color)];
        self.position = origins[self.rng.below(origins.len())?];
        self.target_moves = self.rng.below(21)? as u8 + 20;
        self.solution = path(&self.cells, usize::from(self.position), self.target_moves);
        self.exit = *self.solution.last().ok_or(Error::InvalidPluginInput)?;
        Ok(())
    }
    fn cell(&self) -> Vec<u8> {
        let cell = self.cells[usize::from(self.position)];
        // Source changes exit CellData.Color to Blue, but keeps ColorCells' old
        // membership/order for the Logic UI. The two views must remain separate.
        vec![
            WALL_CODES[usize::from(cell.open)],
            if self.position == self.exit {
                0
            } else {
                cell.color
            },
        ]
    }
    fn logic_data(&self, actions: &mut Actions) {
        let mut walls = Vec::with_capacity(CELLS / 2);
        for row in 0..ROWS {
            for column in 0..COLUMNS {
                if row % 2 == column % 2 {
                    walls.push(WALL_CODES[usize::from(self.cells[row * COLUMNS + column].open)]);
                }
            }
        }
        actions.binary(0, "TSOMaze_Mark_Walls", walls);
        for (color, event) in COLOR_EVENTS.iter().enumerate() {
            let mut coordinates = Vec::with_capacity(self.color_cells[color].len() * 2);
            for cell in &self.color_cells[color] {
                coordinates.push((*cell as usize / COLUMNS) as u8);
                coordinates.push((*cell as usize % COLUMNS) as u8);
            }
            actions.binary(0, event, coordinates);
        }
        actions.binary(
            0,
            "TSOMaze_ExitIcon",
            vec![
                (self.exit as usize / COLUMNS) as u8,
                (self.exit as usize % COLUMNS) as u8,
            ],
        );
    }
    fn solution_data(&self, actions: &mut Actions) {
        let mut coordinates = Vec::with_capacity(1 + (self.solution.len() - 1) * 2);
        coordinates.push(self.origin_color);
        for cell in &self.solution[..self.solution.len() - 1] {
            coordinates.push((*cell as usize / COLUMNS) as u8);
            coordinates.push((*cell as usize % COLUMNS) as u8);
        }
        actions.binary(0, "TSOMaze_Draw_Solution", coordinates);
    }
    fn goto(&mut self, phase: u8, roster: &Roster, actions: &mut Actions) -> Result<(), Error> {
        match phase {
            0 => {
                self.reset()?;
                self.phase = 0;
                actions.broadcast_binary(roster, "TSOMaze_Show_Waiting", vec![]);
            }
            1 => {
                self.tock = 0;
                self.remaining = 10;
                self.phase = 1;
            }
            2 => {
                self.tock = 0;
                self.remaining = 300;
                self.phase = 2;
                if roster[1] != 0 {
                    self.charisma_initialized = true;
                    actions.binary(1, "TSOMaze_Update_Cell", self.cell());
                }
            }
            3 => {
                actions.object(if self.solved {
                    GameObjectEvent::MazeSuccess
                } else {
                    GameObjectEvent::MazeFailure
                });
                actions.broadcast_binary(
                    roster,
                    "TSOMaze_Show_Result",
                    vec![u8::from(self.solved)],
                );
                if self.solved && roster[0] != 0 {
                    self.solution_data(actions);
                }
                self.tock = 0;
                self.remaining = 10;
                self.phase = 3;
            }
            _ => return Err(Error::InvalidPluginInput),
        }
        Ok(())
    }
    pub(crate) fn join(&mut self, role: MazeRole, roster: &Roster) -> Actions {
        let mut actions = Actions::default();
        match role {
            MazeRole::Logic => {
                actions.binary(0, "TSOMaze_Show_Logic", vec![]);
                self.cooldown = 6;
            }
            MazeRole::Charisma => {
                actions.binary(1, "TSOMaze_Show_Charisma", vec![]);
                self.cooldown = 8;
            }
        }
        // Native MAZE-REJOIN-GUARD: a replacement arriving before the queued
        // leave reset is applied must not revive the previous map/solution.
        if roster[0] != 0 && roster[1] != 0 && self.pending != Some(0) {
            self.pending = Some(1);
        }
        actions
    }
    pub(crate) fn message(&mut self, seat: usize, body: &[u8]) -> Actions {
        let mut actions = Actions::default();
        // Native MAZE-PHASE-GUARD also disables movement after a leave or win has
        // queued its transition. This closes the source's between-ticks race.
        if seat != 1 || body.len() != 1 || self.phase != 2 || self.pending.is_some() {
            return actions;
        }
        let direction = usize::from(body[0]);
        if direction < 4
            && self.cells[usize::from(self.position)].open & (1 << direction) != 0
            && let Some(next) = neighbor(usize::from(self.position), direction)
        {
            self.position = next as u16;
            if self.position == self.exit {
                self.solved = true;
                self.pending = Some(3);
            }
        }
        actions.binary(
            1,
            if self.solved {
                "TSOMaze_Final_Cell"
            } else {
                "TSOMaze_Update_Cell"
            },
            self.cell(),
        );
        actions
    }
    pub(crate) fn tick(&mut self, roster: &Roster) -> Result<Actions, Error> {
        let mut actions = Actions::default();
        if let Some(phase) = self.pending.take() {
            self.goto(phase, roster, &mut actions)?;
        }
        match self.phase {
            0 if roster[0] != 0 => {
                if !self.logic_initialized {
                    self.logic_data(&mut actions);
                    self.logic_initialized = true;
                }
                if roster[1] != 0 {
                    if !self.charisma_initialized {
                        actions.binary(1, "TSOMaze_Init_Cell", self.cell());
                        self.charisma_initialized = true;
                    }
                    self.pending = Some(1);
                }
            }
            1 => {
                if self.logic_initialized && self.charisma_initialized {
                    if self.cooldown > 0 {
                        self.tock += 1;
                        if self.tock >= 30 {
                            self.cooldown -= 1;
                            self.tock = 0;
                            actions.broadcast_binary(
                                roster,
                                "TSOMaze_Update_Timer",
                                i32::from(self.cooldown).to_le_bytes().to_vec(),
                            );
                        }
                    } else {
                        self.pending = Some(2);
                    }
                } else if !self.logic_initialized && roster[0] != 0 {
                    self.logic_data(&mut actions);
                    self.logic_initialized = true;
                } else if roster[1] != 0 {
                    actions.binary(1, "TSOMaze_Init_Cell", self.cell());
                    self.charisma_initialized = true;
                }
            }
            2 | 3 => {
                if self.remaining > 0 {
                    self.tock += 1;
                    if self.tock >= 30 {
                        self.remaining -= 1;
                        self.tock = 0;
                        if self.phase == 2 {
                            actions.broadcast_binary(
                                roster,
                                "TSOMaze_Update_Timer",
                                i32::from(self.remaining).to_le_bytes().to_vec(),
                            );
                        }
                    }
                } else {
                    self.pending = Some(if self.phase == 2 { 3 } else { 0 });
                }
            }
            _ => {}
        }
        Ok(actions)
    }
    pub(crate) fn leave(&mut self, roster: &Roster) -> Actions {
        let mut actions = Actions::default();
        if roster[..2] == [0, 0] {
            actions.shutdown = true;
        } else {
            self.pending = Some(0);
        }
        actions
    }
    pub(crate) fn rebind(&self, seat: usize) -> Actions {
        let mut actions = Actions::default();
        if seat == 0 {
            actions.binary(0, "TSOMaze_Show_Logic", vec![]);
            if self.logic_initialized {
                self.logic_data(&mut actions);
            }
        } else {
            actions.binary(1, "TSOMaze_Show_Charisma", vec![]);
            if self.charisma_initialized {
                actions.binary(
                    1,
                    if self.phase == 2 && self.pending.is_none() {
                        "TSOMaze_Update_Cell"
                    } else if self.solved {
                        "TSOMaze_Final_Cell"
                    } else {
                        "TSOMaze_Init_Cell"
                    },
                    self.cell(),
                );
            }
        }
        if self.phase == 0 {
            actions.binary(seat, "TSOMaze_Show_Waiting", vec![]);
        }
        if self.phase == 1 || self.phase == 2 {
            actions.binary(
                seat,
                "TSOMaze_Update_Timer",
                i32::from(if self.phase == 1 {
                    self.cooldown
                } else {
                    self.remaining
                })
                .to_le_bytes()
                .to_vec(),
            );
        }
        if self.phase == 3 {
            actions.binary(seat, "TSOMaze_Show_Result", vec![u8::from(self.solved)]);
            if seat == 0 && self.solved {
                self.solution_data(&mut actions);
            }
        }
        actions
    }
    pub(crate) fn save(&self, writer: &mut Writer) {
        writer.u8(1);
        writer.u8(self.phase);
        writer.u8(self.pending.unwrap_or(255));
        writer.u16(self.cooldown);
        writer.u16(self.remaining);
        writer.u8(self.tock);
        writer.bool(self.logic_initialized);
        writer.bool(self.charisma_initialized);
        writer.bool(self.solved);
        writer.u16(self.position);
        writer.u8(self.origin_color);
        writer.u8(self.target_moves);
        writer.u16(self.exit);
        self.rng.save(writer);
        self.maze_rng.save(writer);
        for cell in self.cells {
            writer.u8(cell.open);
            writer.u8(cell.color);
        }
        for cells in &self.color_cells {
            writer.u8(cells.len() as u8);
            for cell in cells {
                writer.u16(*cell);
            }
        }
        writer.u8(self.solution.len() as u8);
        for cell in &self.solution {
            writer.u16(*cell);
        }
    }
    pub(crate) fn restore(reader: &mut Reader<'_>) -> Result<Self, Error> {
        if reader.u8()? != 1 {
            return Err(Error::InvalidCheckpoint);
        }
        let phase = reader.u8()?;
        let pending = match reader.u8()? {
            255 => None,
            value @ 0..=3 => Some(value),
            _ => return Err(Error::InvalidCheckpoint),
        };
        let cooldown = reader.u16()?;
        let remaining = reader.u16()?;
        let tock = reader.u8()?;
        let logic_initialized = reader.bool()?;
        let charisma_initialized = reader.bool()?;
        let solved = reader.bool()?;
        let position = reader.u16()?;
        let origin_color = reader.u8()?;
        let target_moves = reader.u8()?;
        let exit = reader.u16()?;
        let rng = NativeRng::restore(reader)?;
        let maze_rng = NativeRng::restore(reader)?;
        let pending_valid = match phase {
            0 => matches!(pending, None | Some(0 | 1)),
            1 => matches!(pending, None | Some(0 | 2)),
            2 => matches!(pending, None | Some(0 | 3)),
            3 => matches!(pending, None | Some(0)),
            _ => false,
        };
        if phase > 3
            || cooldown > 8
            || remaining > 300
            || tock >= 30
            || position as usize >= CELLS
            || exit as usize >= CELLS
            || origin_color > 3
            || !(20..=40).contains(&target_moves)
            || (phase == 3 && remaining > 10)
            || (solved && position != exit)
            || (!solved && position == exit)
            || !pending_valid
            || (charisma_initialized && !logic_initialized)
            || (phase == 1 && remaining != 10)
            || (phase < 2 && solved)
            || (phase >= 2 && (!logic_initialized || !charisma_initialized))
            || (pending == Some(2)
                && (cooldown != 0 || !logic_initialized || !charisma_initialized))
            || (phase == 2 && solved && !matches!(pending, Some(0 | 3)))
            || (phase == 2 && pending == Some(3) && !solved && remaining != 0)
            || (phase >= 2 && pending != Some(0) && cooldown != 0)
        {
            return Err(Error::InvalidCheckpoint);
        }
        let mut cells = [Cell { open: 0, color: 4 }; CELLS];
        for cell in &mut cells {
            cell.open = reader.u8()?;
            cell.color = reader.u8()?;
            if cell.open == 0 || cell.open > 15 || cell.color > 4 {
                return Err(Error::InvalidCheckpoint);
            }
        }
        let mut color_cells: [Vec<u16>; 4] = std::array::from_fn(|_| Vec::with_capacity(21));
        let mut colored = [false; CELLS];
        let mut extra_colors = 0;
        for (color, list) in color_cells.iter_mut().enumerate() {
            let count = usize::from(reader.u8()?);
            let base = if color < 2 { 14 } else { 15 };
            if count != base && count != base + 6 {
                return Err(Error::InvalidCheckpoint);
            }
            if count == base + 6 {
                extra_colors += 1;
            }
            for _ in 0..count {
                let cell = reader.u16()?;
                if cell as usize >= CELLS
                    || colored[usize::from(cell)]
                    || cells[usize::from(cell)].color as usize != color
                {
                    return Err(Error::InvalidCheckpoint);
                }
                colored[usize::from(cell)] = true;
                list.push(cell);
            }
        }
        if extra_colors > 1
            || cells
                .iter()
                .enumerate()
                .any(|(index, cell)| colored[index] != (cell.color < 4))
        {
            return Err(Error::InvalidCheckpoint);
        }
        let count = usize::from(reader.u8()?);
        if !(2..=41).contains(&count) {
            return Err(Error::InvalidCheckpoint);
        }
        let mut solution = Vec::with_capacity(41);
        for _ in 0..count {
            let cell = reader.u16()?;
            if cell as usize >= CELLS {
                return Err(Error::InvalidCheckpoint);
            }
            solution.push(cell);
        }
        let mut edge_count = 0;
        for (index, cell) in cells.iter().enumerate() {
            for (direction, opposite) in OPPOSITE.iter().enumerate() {
                if cell.open & (1 << direction) != 0 {
                    let other = neighbor(index, direction).ok_or(Error::InvalidCheckpoint)?;
                    if cells[other].open & (1 << opposite) == 0 {
                        return Err(Error::InvalidCheckpoint);
                    }
                    edge_count += 1;
                }
            }
        }
        if edge_count != 2 * (CELLS - 1) {
            return Err(Error::InvalidCheckpoint);
        }
        let mut seen = [false; CELLS];
        let mut queue = Vec::with_capacity(CELLS);
        queue.push(0);
        seen[0] = true;
        let mut read = 0;
        while read < queue.len() {
            let current = queue[read];
            read += 1;
            for direction in 0..4 {
                if cells[current].open & (1 << direction) != 0 {
                    let next = neighbor(current, direction).ok_or(Error::InvalidCheckpoint)?;
                    if !seen[next] {
                        seen[next] = true;
                        queue.push(next);
                    }
                }
            }
        }
        if queue.len() != CELLS
            || cells[usize::from(solution[0])].color != origin_color
            || (phase < 2 && position != solution[0])
            || solution.last() != Some(&exit)
            || solution != path(&cells, usize::from(solution[0]), target_moves)
        {
            return Err(Error::InvalidCheckpoint);
        }
        Ok(Self {
            phase,
            pending,
            cooldown,
            remaining,
            tock,
            logic_initialized,
            charisma_initialized,
            solved,
            position,
            origin_color,
            target_moves,
            exit,
            cells,
            color_cells,
            solution,
            rng,
            maze_rng,
        })
    }
    pub(crate) fn validate_roster(&self, roster: &Roster) -> bool {
        roster[2..] == [0, 0]
            && ((self.phase == 0 && self.pending != Some(1))
                || self.pending == Some(0)
                || (roster[0] != 0 && roster[1] != 0))
    }
}
