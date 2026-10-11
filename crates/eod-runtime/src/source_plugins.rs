// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
// Translated from FreeSO Signs, Scoreboard and PermissionDoor EOD handlers;
// original FreeSO contributors. Native authorization and bounded parsing apply.

use std::fmt;

// Bounds are checked before any input-derived allocation. A maximum-length
// legacy sign has 32767 UTF-16 units; even three UTF-8 bytes per unit fit here.
const MAX_SOURCE_BYTES: usize = 128 * 1024;
const MAX_PRIVATE_BYTES: usize = MAX_SOURCE_BYTES + 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SignsMode {
    Erase = 0,
    Write = 1,
    Read = 2,
    OwnerPermissions = 3,
    OwnerWrite = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DoorMode {
    Edit = 0,
    View = 1,
    CodeInput = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignsInput {
    pub mode: SignsMode,
    pub max_length: u16,
    pub is_roommate: bool,
    pub owner_authorized: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DoorInput {
    pub mode: DoorMode,
    pub max_fee: i16,
    pub permission_state: i16,
    pub door_fee: i16,
    pub flags: i16,
    pub edit_authorized: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceError {
    InvalidInput,
    InvalidData,
    NotReady,
    NotAuthorized,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum SourceUi {
    Text(&'static str, String),
    Binary(&'static str, Vec<u8>),
}
impl fmt::Debug for SourceUi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SourceUi([REDACTED])")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceObjectEvent {
    SignsWriting(bool),
    ScoreboardScore { team: u8, score: i16 },
    ScoreboardColor { team: u8, color: u8 },
    DoorSave,
    DoorState(i16),
    DoorFee(i16),
    DoorFlags(i16),
    DoorValidation(bool),
}
impl SourceObjectEvent {
    pub fn source_event(&self) -> (i16, Vec<i16>) {
        match *self {
            Self::SignsWriting(writing) => (1, vec![i16::from(writing)]),
            Self::ScoreboardScore { team, score } => (if team == 1 { 1 } else { 2 }, vec![score]),
            Self::ScoreboardColor { team, color } => {
                (if team == 1 { 3 } else { 4 }, vec![i16::from(color)])
            }
            Self::DoorSave => (1, vec![]),
            Self::DoorState(state) => (2, vec![state]),
            Self::DoorFee(fee) => (3, vec![fee]),
            Self::DoorFlags(flags) => (4, vec![flags]),
            Self::DoorValidation(valid) => (7, vec![i16::from(valid)]),
        }
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct Actions {
    pub(crate) ui: Vec<SourceUi>,
    pub(crate) events: Vec<SourceObjectEvent>,
    pub(crate) persist: Option<Vec<u8>>,
    pub(crate) close: bool,
}
impl fmt::Debug for Actions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SourceActions([REDACTED])")
    }
}

#[derive(Clone)]
pub(crate) struct Signs {
    input: SignsInput,
    mode: SignsMode,
    data: Option<SignsData>,
    initialized: bool,
    disable_read: bool,
}
#[derive(Clone)]
pub(crate) struct Scoreboard {
    data: Option<ScoreboardData>,
}
#[derive(Clone)]
pub(crate) struct PermissionDoor {
    input: DoorInput,
    code: Option<u32>,
    initialized: bool,
}

macro_rules! redacted_debug {
    ($($name:ident),+ $(,)?) => {$(
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "([REDACTED])"))
            }
        }
    )+};
}
redacted_debug!(Signs, Scoreboard, PermissionDoor);

#[derive(Clone)]
struct SignsData {
    flags: u16,
    text: String,
}

impl Default for SignsData {
    fn default() -> Self {
        Self {
            flags: 15,
            text: String::new(),
        }
    }
}

impl SignsData {
    // Legacy VMEODSignsData: UInt16 flags, then BinaryWriter's 7-bit UTF-8 length.
    // Invalid UTF-8 is rejected at the native boundary. Source trailing bytes
    // are ignored; private checkpoints require exact field consumption.
    fn decode(bytes: &[u8]) -> Result<Self, SourceError> {
        Self::decode_inner(bytes, false)
    }

    fn decode_inner(bytes: &[u8], exact: bool) -> Result<Self, SourceError> {
        if bytes.len() > MAX_SOURCE_BYTES {
            return Err(SourceError::InvalidData);
        }
        let mut reader = Reader::new(bytes);
        let flags = reader.u16()?;
        let length = reader.string_length()?;
        if length > MAX_SOURCE_BYTES {
            return Err(SourceError::InvalidData);
        }
        let text =
            std::str::from_utf8(reader.take(length)?).map_err(|_| SourceError::InvalidData)?;
        if exact {
            reader.finish()?;
        }
        Ok(Self {
            flags,
            text: text.to_owned(),
        })
    }

    fn encode(&self) -> Vec<u8> {
        let mut bytes = self.flags.to_le_bytes().to_vec();
        write_string_length(&mut bytes, self.text.len());
        bytes.extend_from_slice(self.text.as_bytes());
        bytes
    }
}

#[derive(Clone, Copy)]
struct ScoreboardData {
    colors: [u8; 2],
    scores: [i16; 2],
}

impl Default for ScoreboardData {
    fn default() -> Self {
        Self {
            colors: [0, 1],
            scores: [0, 0],
        }
    }
}

impl ScoreboardData {
    fn decode(bytes: &[u8]) -> Result<Self, SourceError> {
        if bytes.len() < 6 || bytes.len() > MAX_SOURCE_BYTES {
            return Err(SourceError::InvalidData);
        }
        Ok(Self {
            colors: [bytes[0], bytes[1]],
            scores: [
                i16::from_le_bytes([bytes[2], bytes[3]]),
                i16::from_le_bytes([bytes[4], bytes[5]]),
            ],
        })
    }
    fn encode(&self) -> Vec<u8> {
        let mut bytes = self.colors.to_vec();
        for score in self.scores {
            bytes.extend_from_slice(&score.to_le_bytes());
        }
        bytes
    }
    fn show(&self) -> SourceUi {
        SourceUi::Binary("scoreboard_state", self.encode())
    }
}

impl Signs {
    /// Validate a private checkpoint against its provider snapshot or exact
    /// pending candidate. Read denial may clear only the initial session copy.
    pub(crate) fn matches_persistence(&self, bytes: Option<&[u8]>, pending: bool) -> bool {
        let Some(current) = &self.data else {
            return bytes.is_none() && !pending;
        };
        let candidate = match bytes {
            Some(bytes) => match SignsData::decode(bytes) {
                Ok(data) => data,
                Err(_) => return false,
            },
            None if !pending => SignsData::default(),
            None => return false,
        };
        if pending
            && (!self.initialized
                || !matches!(
                    self.mode,
                    SignsMode::Write | SignsMode::OwnerPermissions | SignsMode::OwnerWrite
                )
                || (is_owner_mode(self.mode) && !self.input.owner_authorized)
                || candidate.text.encode_utf16().count() > usize::from(self.input.max_length)
                || bytes != Some(candidate.encode().as_slice()))
        {
            return false;
        }
        current.flags == candidate.flags
            && (current.text == candidate.text
                || (!pending && self.initialized && self.disable_read && current.text.is_empty()))
    }

    /// Source Write always copies the previously loaded flags, even when the
    /// actor also has owner authority. Validate the baseline-to-intent change.
    pub(crate) fn preserves_write_permissions(
        &self,
        baseline: Option<&[u8]>,
        pending: &[u8],
    ) -> bool {
        if self.mode != SignsMode::Write {
            return true;
        }
        let before = match baseline {
            Some(bytes) => match SignsData::decode(bytes) {
                Ok(data) => data,
                Err(_) => return false,
            },
            None => SignsData::default(),
        };
        SignsData::decode(pending).is_ok_and(|after| before.flags == after.flags)
    }

    pub(crate) fn canonical_write(bytes: &[u8]) -> bool {
        match SignsData::decode(bytes) {
            Ok(data) => {
                data.text.encode_utf16().count() <= i16::MAX as usize && data.encode() == bytes
            }
            Err(_) => false,
        }
    }

    pub(crate) fn is_loaded(&self) -> bool {
        self.data.is_some()
    }

    pub(crate) fn new(input: SignsInput) -> Result<Self, SourceError> {
        if input.max_length > i16::MAX as u16 {
            return Err(SourceError::InvalidInput);
        }
        if is_owner_mode(input.mode) && !input.owner_authorized {
            return Err(SourceError::NotAuthorized);
        }
        Ok(Self {
            input,
            mode: input.mode,
            data: None,
            initialized: false,
            disable_read: false,
        })
    }

    pub(crate) fn load(&mut self, bytes: Option<&[u8]>) -> Result<Actions, SourceError> {
        if self.is_loaded() {
            return Err(SourceError::InvalidInput);
        }
        let data = match bytes {
            Some(bytes) => SignsData::decode(bytes)?,
            None => SignsData::default(),
        };
        self.data = Some(data);
        Ok(Actions::default())
    }

    pub(crate) fn tick(&mut self) -> Actions {
        if self.initialized {
            return Actions::default();
        }
        let Some(data) = self.data.as_mut() else {
            return Actions::default();
        };
        if !is_owner_mode(self.mode) {
            let read = if self.input.is_roommate { 1 } else { 4 };
            self.disable_read = data.flags & read == 0;
            // Preserve the source order: the write test can replace its Read mode
            // even when read permission was denied. Friendship is not resolved.
            self.mode = if data.flags & (read << 3) == 0 {
                SignsMode::Read
            } else {
                SignsMode::Write
            };
        }
        // This clears only the loaded session copy, as in the source; no save is
        // emitted by initialization. A later accepted edit persists its own text.
        if self.disable_read {
            data.text.clear();
        }
        self.initialized = true;
        self.show()
    }

    fn show(&self) -> Actions {
        let Some(data) = self.data.as_ref() else {
            return Actions::default();
        };
        let mut visible = data.clone();
        if self.disable_read {
            visible.text.clear();
        }
        Actions {
            ui: vec![
                SourceUi::Text(
                    "signs_init",
                    format!("{}\n{}", self.mode as u8, self.input.max_length),
                ),
                SourceUi::Binary("signs_show", visible.encode()),
            ],
            ..Actions::default()
        }
    }

    pub(crate) fn rebind(&mut self) -> Actions {
        if self.initialized {
            self.show()
        } else {
            self.tick()
        }
    }

    pub(crate) fn message(&mut self, event: &str, payload: &[u8]) -> Result<Actions, SourceError> {
        if event == "close" {
            return Ok(Actions {
                close: true,
                ..Actions::default()
            });
        }
        if event != "set_message" {
            return Err(SourceError::InvalidInput);
        }
        if !self.initialized {
            return Err(SourceError::NotReady);
        }
        let mut data = SignsData::decode(payload)?;
        if self.mode == SignsMode::Read {
            return Ok(Actions::default());
        }
        if self.mode == SignsMode::Write {
            data.flags = self.data.as_ref().ok_or(SourceError::NotReady)?.flags;
        }
        data.text = truncate_utf16(&data.text, usize::from(self.input.max_length))?;
        let output = Actions {
            events: vec![SourceObjectEvent::SignsWriting(!data.text.is_empty())],
            persist: Some(data.encode()),
            ..Actions::default()
        };
        self.data = Some(data);
        Ok(output)
    }

    pub(crate) fn save_private(&self) -> Vec<u8> {
        let mut bytes = b"SGN1".to_vec();
        bytes.push(self.input.mode as u8);
        bytes.extend_from_slice(&self.input.max_length.to_le_bytes());
        bytes.extend_from_slice(&[
            u8::from(self.input.is_roommate),
            u8::from(self.input.owner_authorized),
            self.mode as u8,
            u8::from(self.initialized),
            u8::from(self.disable_read),
            u8::from(self.data.is_some()),
        ]);
        if let Some(data) = &self.data {
            bytes.extend_from_slice(&data.encode());
        }
        bytes
    }

    pub(crate) fn restore_private(bytes: &[u8]) -> Result<Self, SourceError> {
        let mut reader = private_reader(bytes, b"SGN1")?;
        let input = SignsInput {
            mode: signs_mode(reader.byte()?)?,
            max_length: reader.u16()?,
            is_roommate: reader.boolean()?,
            owner_authorized: reader.boolean()?,
        };
        let mut state = Self::new(input).map_err(|_| SourceError::InvalidData)?;
        state.mode = signs_mode(reader.byte()?)?;
        state.initialized = reader.boolean()?;
        state.disable_read = reader.boolean()?;
        if reader.boolean()? {
            state.data = Some(SignsData::decode_inner(reader.remaining(), true)?);
            reader.take(reader.remaining().len())?;
        }
        reader.finish()?;
        if state.initialized {
            let data = state.data.as_ref().ok_or(SourceError::InvalidData)?;
            if is_owner_mode(input.mode) {
                if state.mode != input.mode || state.disable_read {
                    return Err(SourceError::InvalidData);
                }
            } else {
                let read = if input.is_roommate { 1 } else { 4 };
                let mode = if data.flags & (read << 3) == 0 {
                    SignsMode::Read
                } else {
                    SignsMode::Write
                };
                if state.mode != mode || state.disable_read != (data.flags & read == 0) {
                    return Err(SourceError::InvalidData);
                }
            }
        } else if state.mode != input.mode || state.disable_read {
            return Err(SourceError::InvalidData);
        }
        Ok(state)
    }
}
impl Scoreboard {
    pub(crate) fn matches_persistence(&self, bytes: Option<&[u8]>, pending: bool) -> bool {
        let Some(current) = self.data else {
            return bytes.is_none() && !pending;
        };
        let candidate = match bytes {
            Some(bytes) => match ScoreboardData::decode(bytes) {
                Ok(data) => data,
                Err(_) => return false,
            },
            None if !pending => ScoreboardData::default(),
            None => return false,
        };
        (!pending || bytes == Some(candidate.encode().as_slice()))
            && current.colors == candidate.colors
            && current.scores == candidate.scores
    }

    pub(crate) fn canonical_write(bytes: &[u8]) -> bool {
        match ScoreboardData::decode(bytes) {
            Ok(data) => data.encode() == bytes,
            Err(_) => false,
        }
    }

    pub(crate) fn is_loaded(&self) -> bool {
        self.data.is_some()
    }

    pub(crate) fn new() -> Result<Self, SourceError> {
        Ok(Self { data: None })
    }

    pub(crate) fn load(&mut self, bytes: Option<&[u8]>) -> Result<Actions, SourceError> {
        if self.is_loaded() {
            return Err(SourceError::InvalidInput);
        }
        let data = match bytes {
            Some(bytes) => ScoreboardData::decode(bytes)?,
            None => ScoreboardData::default(),
        };
        self.data = Some(data);
        Ok(Actions {
            ui: vec![data.show()],
            ..Actions::default()
        })
    }

    // The host supplies scoreboard_show immediately on initial connection.
    pub(crate) fn tick(&mut self) -> Actions {
        Actions::default()
    }

    pub(crate) fn rebind(&mut self) -> Actions {
        let mut ui = vec![SourceUi::Text("scoreboard_show", String::new())];
        if let Some(data) = &self.data {
            ui.push(data.show());
        }
        Actions {
            ui,
            ..Actions::default()
        }
    }

    pub(crate) fn message(&mut self, event: &str, payload: &[u8]) -> Result<Actions, SourceError> {
        if event == "close" {
            return Ok(Actions {
                close: true,
                ..Actions::default()
            });
        }
        if !matches!(
            event,
            "scoreboard_setscore" | "scoreboard_updatescore" | "scoreboard_updatecolor"
        ) {
            return Err(SourceError::InvalidInput);
        }
        let mut data = self.data.ok_or(SourceError::NotReady)?;
        let body = bounded_text(payload)?;
        let mut parts = body.split(',');
        let team = parse_team(parts.next().ok_or(SourceError::InvalidData)?)?;
        let value = parts.next().ok_or(SourceError::InvalidData)?;
        if parts.next().is_some() {
            return Err(SourceError::InvalidData);
        }
        let mut events = Vec::new();
        let mut ui = Vec::new();
        if event == "scoreboard_updatecolor" {
            let color = parse_color(value)?;
            if let 1..=2 = team {
                data.colors[usize::from(team - 1)] = color;
                events.push(SourceObjectEvent::ScoreboardColor { team, color });
            }
        } else {
            let value = parse_integer::<i16>(value)?;
            if let 1..=2 = team {
                let index = usize::from(team - 1);
                // C# compound assignment narrows to Int16 before both clamps.
                let score = if event == "scoreboard_updatescore" {
                    data.scores[index].wrapping_add(value)
                } else {
                    value
                }
                .clamp(0, 999);
                data.scores[index] = score;
                events.push(SourceObjectEvent::ScoreboardScore { team, score });
            }
            ui.push(data.show());
        }
        let output = Actions {
            ui,
            events,
            persist: Some(data.encode()),
            close: false,
        };
        self.data = Some(data);
        Ok(output)
    }

    pub(crate) fn save_private(&self) -> Vec<u8> {
        let mut bytes = b"SCB1".to_vec();
        bytes.push(u8::from(self.data.is_some()));
        if let Some(data) = self.data {
            bytes.extend_from_slice(&data.encode());
        }
        bytes
    }

    pub(crate) fn restore_private(bytes: &[u8]) -> Result<Self, SourceError> {
        let mut reader = private_reader(bytes, b"SCB1")?;
        let data = if reader.boolean()? {
            Some(ScoreboardData::decode(reader.take(6)?)?)
        } else {
            None
        };
        reader.finish()?;
        Ok(Self { data })
    }
}
impl PermissionDoor {
    pub(crate) fn matches_persistence(&self, bytes: Option<&[u8]>, pending: bool) -> bool {
        let Some(cached_code) = self.code else {
            return bytes.is_none() && !pending;
        };
        if bytes.is_some_and(|bytes| bytes.len() > MAX_SOURCE_BYTES) {
            return false;
        }
        if pending {
            if self.input.mode != DoorMode::Edit || !self.input.edit_authorized {
                return false;
            }
            let Some(bytes) = bytes else {
                return false;
            };
            return parse_door_code(bytes).is_ok_and(|code| code.to_string().as_bytes() == bytes);
        }
        // Edit intentionally keeps the code loaded at connection time even after
        // successful writes replace provider data. View/Input never write, so
        // their cached value must match the source's current parse-zero fallback.
        self.input.mode == DoorMode::Edit
            || cached_code
                == bytes
                    .and_then(|bytes| parse_source_u32(bytes).ok())
                    .unwrap_or(0)
    }

    pub(crate) fn is_loaded(&self) -> bool {
        self.code.is_some()
    }

    pub(crate) fn new(input: DoorInput) -> Result<Self, SourceError> {
        if input.mode == DoorMode::Edit && !input.edit_authorized {
            return Err(SourceError::NotAuthorized);
        }
        Ok(Self {
            input,
            code: None,
            initialized: false,
        })
    }

    pub(crate) fn load(&mut self, bytes: Option<&[u8]>) -> Result<Actions, SourceError> {
        if self.is_loaded() {
            return Err(SourceError::InvalidInput);
        }
        if bytes.is_some_and(|value| value.len() > MAX_SOURCE_BYTES) {
            return Err(SourceError::InvalidData);
        }
        // Source loading accepts any UInt32 and falls back to zero on parse
        // failure; the nine-digit bound applies only to incoming commands.
        self.code = Some(
            bytes
                .and_then(|value| std::str::from_utf8(value).ok())
                .and_then(|value| parse_integer::<u32>(value).ok())
                .unwrap_or(0),
        );
        Ok(Actions::default())
    }

    pub(crate) fn tick(&mut self) -> Actions {
        if self.initialized || self.code.is_none() {
            return Actions::default();
        }
        self.initialized = true;
        self.show()
    }

    fn show(&self) -> Actions {
        let Some(code) = self.code else {
            return Actions::default();
        };
        let mut ui = vec![SourceUi::Text(
            "door_init",
            format!(
                "{}\n{}\n{}\n{}\n{}",
                self.input.mode as u8,
                self.input.max_fee,
                self.input.permission_state,
                self.input.door_fee,
                self.input.flags
            ),
        )];
        if self.input.mode == DoorMode::Edit {
            ui.push(SourceUi::Text("door_code", code.to_string()));
        }
        Actions {
            ui,
            ..Actions::default()
        }
    }

    pub(crate) fn rebind(&mut self) -> Actions {
        if self.initialized {
            self.show()
        } else {
            self.tick()
        }
    }

    pub(crate) fn message(&mut self, event: &str, payload: &[u8]) -> Result<Actions, SourceError> {
        match event {
            "close" => Ok(Actions {
                events: if self.input.mode == DoorMode::Edit {
                    vec![SourceObjectEvent::DoorSave]
                } else {
                    vec![]
                },
                close: true,
                ..Actions::default()
            }),
            "try_code" => {
                if self.input.mode != DoorMode::CodeInput {
                    return Ok(Actions::default());
                }
                let mut output = Actions {
                    close: true,
                    ..Actions::default()
                };
                if self.initialized
                    && let Ok(code) = parse_door_code(payload)
                {
                    output
                        .events
                        .push(SourceObjectEvent::DoorValidation(self.code == Some(code)));
                }
                Ok(output)
            }
            "set_code" | "set_state" | "set_fee" | "set_flags" => {
                if self.input.mode != DoorMode::Edit {
                    return Ok(Actions::default());
                }
                let mut output = Actions::default();
                if event == "set_code" {
                    let code = parse_door_code(payload)?;
                    output.persist = Some(code.to_string().into_bytes());
                    // Source saves the new code without changing its cached Code.
                } else {
                    let value = parse_integer::<u16>(bounded_text(payload)?)?;
                    let object_event = match event {
                        "set_state" if value <= 2 => SourceObjectEvent::DoorState(value as i16),
                        "set_fee" if i32::from(value) <= i32::from(self.input.max_fee) => {
                            SourceObjectEvent::DoorFee(value as i16)
                        }
                        "set_flags" => SourceObjectEvent::DoorFlags((value & 252) as i16),
                        _ => return Err(SourceError::InvalidData),
                    };
                    output.events.push(object_event);
                    // Object events own these changes; source init registers stay
                    // unchanged in the handler, including on private restoration.
                }
                Ok(output)
            }
            _ => Err(SourceError::InvalidInput),
        }
    }

    pub(crate) fn save_private(&self) -> Vec<u8> {
        let mut bytes = b"DOR1".to_vec();
        bytes.push(self.input.mode as u8);
        for value in [
            self.input.max_fee,
            self.input.permission_state,
            self.input.door_fee,
            self.input.flags,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&[
            u8::from(self.input.edit_authorized),
            u8::from(self.initialized),
            u8::from(self.code.is_some()),
        ]);
        if let Some(code) = self.code {
            bytes.extend_from_slice(&code.to_le_bytes());
        }
        bytes
    }

    pub(crate) fn restore_private(bytes: &[u8]) -> Result<Self, SourceError> {
        let mut reader = private_reader(bytes, b"DOR1")?;
        let input = DoorInput {
            mode: door_mode(reader.byte()?)?,
            max_fee: reader.i16()?,
            permission_state: reader.i16()?,
            door_fee: reader.i16()?,
            flags: reader.i16()?,
            edit_authorized: reader.boolean()?,
        };
        let mut state = Self::new(input).map_err(|_| SourceError::InvalidData)?;
        state.initialized = reader.boolean()?;
        if reader.boolean()? {
            state.code = Some(reader.u32()?);
        }
        reader.finish()?;
        if state.initialized && state.code.is_none() {
            return Err(SourceError::InvalidData);
        }
        Ok(state)
    }
}

fn is_owner_mode(mode: SignsMode) -> bool {
    matches!(mode, SignsMode::OwnerPermissions | SignsMode::OwnerWrite)
}

fn signs_mode(value: u8) -> Result<SignsMode, SourceError> {
    match value {
        0 => Ok(SignsMode::Erase),
        1 => Ok(SignsMode::Write),
        2 => Ok(SignsMode::Read),
        3 => Ok(SignsMode::OwnerPermissions),
        4 => Ok(SignsMode::OwnerWrite),
        _ => Err(SourceError::InvalidData),
    }
}

fn door_mode(value: u8) -> Result<DoorMode, SourceError> {
    match value {
        0 => Ok(DoorMode::Edit),
        1 => Ok(DoorMode::View),
        2 => Ok(DoorMode::CodeInput),
        _ => Err(SourceError::InvalidData),
    }
}

fn bounded_text(bytes: &[u8]) -> Result<&str, SourceError> {
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(SourceError::InvalidData);
    }
    std::str::from_utf8(bytes).map_err(|_| SourceError::InvalidData)
}

fn parse_integer<T: TryFrom<i64>>(value: &str) -> Result<T, SourceError> {
    // NumberStyles.Integer permits ASCII space/U+0009..U+000D, an optional
    // sign, and final NULs. Whitespace after the first NUL is not accepted.
    // Enum.TryParse separately applies Unicode Trim before calling this path.
    let value = value
        .trim_end_matches('\0')
        .trim_matches(|ch| matches!(ch, ' ' | '\t'..='\r'));
    let value = value.parse::<i64>().map_err(|_| SourceError::InvalidData)?;
    T::try_from(value).map_err(|_| SourceError::InvalidData)
}

pub(crate) fn parse_source_u32(bytes: &[u8]) -> Result<u32, SourceError> {
    parse_integer(bounded_text(bytes)?)
}

fn parse_team(value: &str) -> Result<u8, SourceError> {
    match value.trim() {
        "LHS" => Ok(1),
        "RHS" => Ok(2),
        value => parse_integer(value),
    }
}

fn parse_color(value: &str) -> Result<u8, SourceError> {
    match value.trim() {
        "Red" => Ok(0),
        "Blue" => Ok(1),
        "Yellow" => Ok(2),
        "Green" => Ok(3),
        "Orange" => Ok(4),
        "Purple" => Ok(5),
        "White" => Ok(6),
        "Black" => Ok(7),
        value => parse_integer(value),
    }
}

fn parse_door_code(payload: &[u8]) -> Result<u32, SourceError> {
    let code = parse_source_u32(payload)?;
    if code > 999_999_999 {
        return Err(SourceError::InvalidData);
    }
    Ok(code)
}

fn truncate_utf16(text: &str, limit: usize) -> Result<String, SourceError> {
    let mut output = String::new();
    let mut remaining = limit;
    for character in text.chars() {
        if remaining == 0 {
            break;
        }
        let units = character.len_utf16();
        if units > remaining {
            // BinaryWriter(Stream) uses strict UTF-8 and throws for the lone
            // surrogate left by C# Substring. Reject without its partial state
            // mutation instead of silently saving different replacement text.
            return Err(SourceError::InvalidData);
        }
        output.push(character);
        remaining -= units;
    }
    Ok(output)
}

fn write_string_length(bytes: &mut Vec<u8>, mut length: usize) {
    while length >= 128 {
        bytes.push((length as u8 & 127) | 128);
        length >>= 7;
    }
    bytes.push(length as u8);
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
    fn take(&mut self, length: usize) -> Result<&'a [u8], SourceError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(SourceError::InvalidData)?;
        let result = self
            .bytes
            .get(self.position..end)
            .ok_or(SourceError::InvalidData)?;
        self.position = end;
        Ok(result)
    }
    fn remaining(&self) -> &'a [u8] {
        &self.bytes[self.position..]
    }
    fn finish(&self) -> Result<(), SourceError> {
        if self.position == self.bytes.len() {
            Ok(())
        } else {
            Err(SourceError::InvalidData)
        }
    }
    fn byte(&mut self) -> Result<u8, SourceError> {
        Ok(self.take(1)?[0])
    }
    fn boolean(&mut self) -> Result<bool, SourceError> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(SourceError::InvalidData),
        }
    }
    fn u16(&mut self) -> Result<u16, SourceError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }
    fn i16(&mut self) -> Result<i16, SourceError> {
        let bytes = self.take(2)?;
        Ok(i16::from_le_bytes([bytes[0], bytes[1]]))
    }
    fn u32(&mut self) -> Result<u32, SourceError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
    fn string_length(&mut self) -> Result<usize, SourceError> {
        let mut length = 0u32;
        for shift in [0, 7, 14, 21, 28] {
            let byte = self.byte()?;
            if shift == 28 && byte > 7 {
                return Err(SourceError::InvalidData);
            }
            length |= u32::from(byte & 127) << shift;
            if byte & 128 == 0 {
                return Ok(length as usize);
            }
        }
        Err(SourceError::InvalidData)
    }
}

fn private_reader<'a>(bytes: &'a [u8], magic: &[u8; 4]) -> Result<Reader<'a>, SourceError> {
    if bytes.len() > MAX_PRIVATE_BYTES {
        return Err(SourceError::InvalidData);
    }
    let mut reader = Reader::new(bytes);
    if reader.take(4)? != magic {
        return Err(SourceError::InvalidData);
    }
    Ok(reader)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign_input(mode: SignsMode) -> SignsInput {
        SignsInput {
            mode,
            max_length: 5,
            is_roommate: true,
            owner_authorized: true,
        }
    }
    fn door_input(mode: DoorMode) -> DoorInput {
        DoorInput {
            mode,
            max_fee: 100,
            permission_state: 1,
            door_fee: 10,
            flags: 44,
            edit_authorized: true,
        }
    }

    // Catches interpreting an invocation mode as authenticated ownership.
    #[test]
    fn privileged_modes_require_authoritative_invocation_permission() {
        for mode in [SignsMode::OwnerPermissions, SignsMode::OwnerWrite] {
            let mut input = sign_input(mode);
            input.owner_authorized = false;
            assert!(matches!(Signs::new(input), Err(SourceError::NotAuthorized)));
        }
        let mut input = sign_input(SignsMode::Write);
        input.max_length = 32768;
        assert!(matches!(Signs::new(input), Err(SourceError::InvalidInput)));
        let mut input = door_input(DoorMode::Edit);
        input.edit_authorized = false;
        assert!(matches!(
            PermissionDoor::new(input),
            Err(SourceError::NotAuthorized)
        ));
    }

    // Catches the original pre-Tick race that allowed writes before permissions.
    #[test]
    fn signs_load_and_initialization_gate_changes() {
        let mut sign = Signs::new(sign_input(SignsMode::Read)).unwrap();
        assert_eq!(sign.tick(), Actions::default());
        assert_eq!(
            sign.message("set_message", &[15, 0, 1, b'x']),
            Err(SourceError::NotReady)
        );
        assert_eq!(sign.load(None).unwrap(), Actions::default());
        assert_eq!(
            sign.message("set_message", &[15, 0, 1, b'x']),
            Err(SourceError::NotReady)
        );
        assert_eq!(
            sign.tick().ui,
            vec![
                SourceUi::Text("signs_init", "1\n5".into()),
                SourceUi::Binary("signs_show", vec![15, 0, 0]),
            ]
        );
        assert_eq!(sign.tick(), Actions::default());
    }

    // Catches accidentally changing source UTF-16 length accounting to scalar count.
    #[test]
    fn signs_writer_preserves_flags_and_counts_supplementary_utf16_units() {
        let mut input = sign_input(SignsMode::Write);
        input.max_length = 3;
        let mut sign = Signs::new(input).unwrap();
        sign.load(None).unwrap();
        sign.tick();
        let output = sign
            .message(
                "set_message",
                &[255, 255, 6, b'A', 0xf0, 0x9f, 0x98, 0x80, b'z'],
            )
            .unwrap();
        assert_eq!(output.events, vec![SourceObjectEvent::SignsWriting(true)]);
        assert_eq!(
            output.persist,
            Some(vec![15, 0, 5, b'A', 0xf0, 0x9f, 0x98, 0x80])
        );
        assert!(output.ui.is_empty());
        let empty = sign.message("set_message", &[255, 255, 0]).unwrap();
        assert_eq!(empty.persist, Some(vec![15, 0, 0]));
        assert_eq!(empty.events, vec![SourceObjectEvent::SignsWriting(false)]);
    }

    // Source BinaryWriter's strict UTF-8 encoder throws on a split surrogate.
    // The native boundary rejects atomically instead of partially changing Data.
    #[test]
    fn signs_rejects_a_split_surrogate_without_mutating_session() {
        let mut input = sign_input(SignsMode::OwnerWrite);
        input.max_length = 2;
        let mut sign = Signs::new(input).unwrap();
        sign.load(Some(&[15, 0, 1, b'x'])).unwrap();
        sign.tick();
        let before = sign.save_private();
        assert_eq!(
            sign.message(
                "set_message",
                &[15, 0, 6, b'A', 0xf0, 0x9f, 0x98, 0x80, b'z']
            ),
            Err(SourceError::InvalidData)
        );
        assert_eq!(sign.save_private(), before);
    }

    // Literal cases were checked against Mono UInt32.TryParse/Enum.TryParse.
    #[test]
    fn numeric_parser_preserves_source_nul_and_whitespace_rules() {
        for (input, expected) in [
            (b"-0".as_slice(), 0),
            (b"-000", 0),
            (b"+0001", 1),
            (b"1\0", 1),
            (b"1 \0", 1),
            (b"1\t\0", 1),
            (b"1\0\0", 1),
            (b"4294967295", u32::MAX),
        ] {
            assert_eq!(parse_source_u32(input), Ok(expected));
        }
        for input in [
            b"1\0 ".as_slice(),
            "\u{00a0}1\u{00a0}".as_bytes(),
            "\u{0085}1\u{0085}".as_bytes(),
            b"-1",
            b"4294967296",
        ] {
            assert_eq!(parse_source_u32(input), Err(SourceError::InvalidData));
        }
        let mut board = Scoreboard::new().unwrap();
        board.load(None).unwrap();
        assert_eq!(
            board
                .message("scoreboard_setscore", "\u{00a0}1\u{00a0},7\0".as_bytes())
                .unwrap()
                .events,
            vec![SourceObjectEvent::ScoreboardScore { team: 1, score: 7 }]
        );
        assert_eq!(
            board.message("scoreboard_setscore", "LHS,\u{00a0}1\u{00a0}".as_bytes()),
            Err(SourceError::InvalidData)
        );
    }

    // Catches exposing a persisted private sign to a reader lacking its read bit.
    #[test]
    fn signs_denied_read_redacts_session_copy_with_source_write_mode_quirk() {
        let mut input = sign_input(SignsMode::Read);
        input.is_roommate = false;
        input.owner_authorized = false;
        let mut sign = Signs::new(input).unwrap();
        sign.load(Some(&[32, 0, 6, b's', b'e', b'c', b'r', b'e', b't']))
            .unwrap();
        let output = sign.tick();
        assert_eq!(
            output.ui,
            vec![
                SourceUi::Text("signs_init", "1\n5".into()),
                SourceUi::Binary("signs_show", vec![32, 0, 0]),
            ]
        );
        assert!(output.persist.is_none());
        assert_eq!(sign.rebind().ui, output.ui);
        let result = sign.message("set_message", &[63, 0, 1, b'x']).unwrap();
        assert_eq!(result.persist, Some(vec![32, 0, 1, b'x']));
    }

    // Catches granting friendship-specific rights without a relationship resolver.
    #[test]
    fn signs_readonly_and_owner_modes_follow_source_permissions() {
        let mut input = sign_input(SignsMode::Erase);
        input.is_roommate = false;
        let mut reader = Signs::new(input).unwrap();
        reader.load(Some(&[20, 0, 1, b'x'])).unwrap();
        assert_eq!(
            reader.tick().ui[0],
            SourceUi::Text("signs_init", "2\n5".into())
        );
        assert_eq!(
            reader.message("set_message", &[63, 0, 1, b'y']).unwrap(),
            Actions::default()
        );
        let mut owner = Signs::new(sign_input(SignsMode::OwnerPermissions)).unwrap();
        owner.load(Some(&[0, 0, 1, b'x'])).unwrap();
        assert_eq!(
            owner.tick().ui[0],
            SourceUi::Text("signs_init", "3\n5".into())
        );
        assert_eq!(
            owner
                .message("set_message", &[255, 255, 1, b'z'])
                .unwrap()
                .persist,
            Some(vec![255, 255, 1, b'z'])
        );
    }

    // Catches partial/malformed reads changing state or allocating from a forged length.
    #[test]
    fn signs_malformed_payloads_leave_state_unchanged() {
        let mut sign = Signs::new(sign_input(SignsMode::OwnerWrite)).unwrap();
        sign.load(None).unwrap();
        sign.tick();
        let before = sign.save_private();
        for bad in [
            vec![],
            vec![15],
            vec![15, 0],
            vec![15, 0, 2, b'x'],
            vec![15, 0, 1, 255],
            vec![15, 0, 255, 255, 255, 255, 127],
        ] {
            assert_eq!(
                sign.message("set_message", &bad),
                Err(SourceError::InvalidData)
            );
            assert_eq!(sign.save_private(), before);
        }
        assert_eq!(sign.message("unknown", b""), Err(SourceError::InvalidInput));
    }

    // Catches replacing the 7-bit UTF-8 byte length with a one-byte character length.
    #[test]
    fn signs_multibyte_string_length_round_trips() {
        let mut input = sign_input(SignsMode::OwnerWrite);
        input.max_length = 200;
        let mut sign = Signs::new(input).unwrap();
        let mut bytes = vec![15, 0, 128, 1];
        bytes.extend_from_slice(&[b'x'; 128]);
        sign.load(Some(&bytes)).unwrap();
        assert_eq!(
            sign.tick().ui[1],
            SourceUi::Binary("signs_show", bytes.clone())
        );
        assert_eq!(
            sign.message("set_message", &bytes).unwrap().persist,
            Some(bytes)
        );
    }

    // Catches changing the six-byte source persistence field order or widths.
    #[test]
    fn scoreboard_load_and_rebind_use_source_binary_state() {
        let mut board = Scoreboard::new().unwrap();
        assert_eq!(
            board.message("scoreboard_setscore", b"LHS,10"),
            Err(SourceError::NotReady)
        );
        assert_eq!(board.tick(), Actions::default());
        assert_eq!(
            board.load(None).unwrap().ui,
            vec![SourceUi::Binary("scoreboard_state", vec![0, 1, 0, 0, 0, 0])]
        );
        assert_eq!(
            board.rebind().ui,
            vec![
                SourceUi::Text("scoreboard_show", "".into()),
                SourceUi::Binary("scoreboard_state", vec![0, 1, 0, 0, 0, 0])
            ]
        );
        assert!(board.message("close", b"").unwrap().close);
    }

    // Catches widening the addition before clamping and losing C# short overflow behavior.
    #[test]
    fn scoreboard_score_updates_wrap_before_source_clamp() {
        let mut board = Scoreboard::new().unwrap();
        board.load(Some(&[0, 1, 231, 3, 2, 0])).unwrap();
        let output = board
            .message("scoreboard_updatescore", b"LHS,32767")
            .unwrap();
        assert_eq!(
            output.events,
            vec![SourceObjectEvent::ScoreboardScore { team: 1, score: 0 }]
        );
        assert_eq!(output.persist, Some(vec![0, 1, 0, 0, 2, 0]));
        assert_eq!(
            output.ui,
            vec![SourceUi::Binary("scoreboard_state", vec![0, 1, 0, 0, 2, 0])]
        );
        assert_eq!(
            board
                .message("scoreboard_setscore", b"RHS,32000")
                .unwrap()
                .events,
            vec![SourceObjectEvent::ScoreboardScore {
                team: 2,
                score: 999
            }]
        );
        assert_eq!(
            board
                .message("scoreboard_setscore", b"RHS,-32768")
                .unwrap()
                .events,
            vec![SourceObjectEvent::ScoreboardScore { team: 2, score: 0 }]
        );
    }

    // Catches rejecting numeric enum values which .NET Enum.TryParse accepts.
    #[test]
    fn scoreboard_numeric_enum_values_preserve_noop_and_color_quirks() {
        let mut board = Scoreboard::new().unwrap();
        board.load(None).unwrap();
        let output = board
            .message("scoreboard_updatecolor", b" +1 ,255")
            .unwrap();
        assert_eq!(
            output.events,
            vec![SourceObjectEvent::ScoreboardColor {
                team: 1,
                color: 255
            }]
        );
        assert!(output.ui.is_empty());
        assert_eq!(output.persist, Some(vec![255, 1, 0, 0, 0, 0]));
        let noop = board.message("scoreboard_setscore", b"3,1").unwrap();
        assert!(noop.events.is_empty());
        assert_eq!(noop.persist, Some(vec![255, 1, 0, 0, 0, 0]));
        assert_eq!(noop.ui.len(), 1);
        let before = board.save_private();
        for bad in [
            b"lhs,1".as_slice(),
            b"LHS,32768",
            b"LHS,1,2",
            b"256,1",
            b"LHS,1.0",
            b"LHS,\xff",
        ] {
            assert_eq!(
                board.message("scoreboard_setscore", bad),
                Err(SourceError::InvalidData)
            );
            assert_eq!(board.save_private(), before);
        }
        assert_eq!(
            board
                .message("scoreboard_updatecolor", b"RHS,Purple")
                .unwrap()
                .events,
            vec![SourceObjectEvent::ScoreboardColor { team: 2, color: 5 }]
        );
    }

    // Catches dropping loaded out-of-range source state before its next valid mutation.
    #[test]
    fn scoreboard_load_preserves_raw_fields_but_rejects_truncation() {
        let bytes = [254, 253, 255, 255, 0, 128];
        let mut board = Scoreboard::new().unwrap();
        assert_eq!(
            board.load(Some(&bytes)).unwrap().ui,
            vec![SourceUi::Binary("scoreboard_state", bytes.to_vec())]
        );
        for bytes in [vec![], vec![0; 5]] {
            assert_eq!(
                Scoreboard::new().unwrap().load(Some(&bytes)),
                Err(SourceError::InvalidData)
            );
        }
    }

    // Catches adding an exact-consumption rule to harmless legacy payload tails.
    #[test]
    fn source_payload_tails_are_ignored_but_not_retained() {
        let mut sign = Signs::new(sign_input(SignsMode::OwnerWrite)).unwrap();
        sign.load(Some(&[15, 0, 1, b'x', 88])).unwrap();
        assert_eq!(
            sign.tick().ui[1],
            SourceUi::Binary("signs_show", vec![15, 0, 1, b'x'])
        );
        assert_eq!(
            sign.message("set_message", &[15, 0, 1, b'y', 99])
                .unwrap()
                .persist,
            Some(vec![15, 0, 1, b'y'])
        );
        let mut board = Scoreboard::new().unwrap();
        assert_eq!(
            board.load(Some(&[0, 1, 1, 0, 2, 0, 88])).unwrap().ui,
            vec![SourceUi::Binary("scoreboard_state", vec![0, 1, 1, 0, 2, 0])]
        );
    }

    // Catches publishing or revealing door codes to view/input clients.
    #[test]
    fn door_code_is_only_in_authorized_private_edit_initialization() {
        for mode in [DoorMode::Edit, DoorMode::View, DoorMode::CodeInput] {
            let mut door = PermissionDoor::new(door_input(mode)).unwrap();
            assert_eq!(door.tick(), Actions::default());
            assert_eq!(door.load(Some(b"123456789")).unwrap(), Actions::default());
            let output = door.tick();
            assert_eq!(
                output.ui[0],
                SourceUi::Text("door_init", format!("{}\n100\n1\n10\n44", mode as u8))
            );
            assert_eq!(output.ui.len(), if mode == DoorMode::Edit { 2 } else { 1 });
            if mode == DoorMode::Edit {
                assert_eq!(
                    output.ui[1],
                    SourceUi::Text("door_code", "123456789".into())
                );
            }
            assert!(output.events.is_empty());
            assert!(!format!("{output:?}").contains("123456789"));
            assert!(!format!("{:?}", output.ui).contains("123456789"));
            assert_eq!(door.tick(), Actions::default());
        }
    }

    // Catches emitting success for an uninitialized or malformed code attempt.
    #[test]
    fn door_code_attempts_close_with_only_the_source_validation_event() {
        let mut door = PermissionDoor::new(door_input(DoorMode::CodeInput)).unwrap();
        let early = door.message("try_code", b"0").unwrap();
        assert!(early.close && early.events.is_empty());
        door.load(Some(b"123")).unwrap();
        door.tick();
        for (input, result) in [
            (b"00123".as_slice(), Some(true)),
            (b"124", Some(false)),
            (b"1000000000", None),
            (b"-1", None),
            (b"x", None),
        ] {
            let output = door.message("try_code", input).unwrap();
            assert!(output.close);
            assert_eq!(
                output.events,
                result
                    .map(SourceObjectEvent::DoorValidation)
                    .into_iter()
                    .collect::<Vec<_>>()
            );
            assert!(output.persist.is_none() && output.ui.is_empty());
        }
    }

    // Catches updating the source's deliberately stale cached Code after saving.
    #[test]
    fn door_set_code_persists_canonical_decimal_without_public_code_event() {
        let mut door = PermissionDoor::new(door_input(DoorMode::Edit)).unwrap();
        door.load(Some(b"123")).unwrap();
        door.tick();
        let output = door.message("set_code", b" +000456 ").unwrap();
        assert_eq!(output.persist, Some(b"456".to_vec()));
        assert!(output.ui.is_empty() && output.events.is_empty() && !output.close);
        assert_eq!(
            door.rebind().ui[1],
            SourceUi::Text("door_code", "123".into())
        );
        assert_eq!(
            door.message("set_code", b"1000000000"),
            Err(SourceError::InvalidData)
        );
    }

    // Catches wrong object event codes, missing flags masking, or saving on View close.
    #[test]
    fn door_edit_actions_validate_ranges_and_mask_flags() {
        let mut door = PermissionDoor::new(door_input(DoorMode::Edit)).unwrap();
        assert_eq!(
            door.message("set_state", b"2").unwrap().events,
            vec![SourceObjectEvent::DoorState(2)]
        );
        assert_eq!(
            door.message("set_fee", b"100").unwrap().events,
            vec![SourceObjectEvent::DoorFee(100)]
        );
        assert_eq!(
            door.message("set_flags", b"65535").unwrap().events,
            vec![SourceObjectEvent::DoorFlags(252)]
        );
        assert_eq!(
            door.message("set_state", b"3"),
            Err(SourceError::InvalidData)
        );
        assert_eq!(
            door.message("set_fee", b"101"),
            Err(SourceError::InvalidData)
        );
        let close = door.message("close", b"").unwrap();
        assert_eq!(close.events, vec![SourceObjectEvent::DoorSave]);
        assert!(close.close && close.persist.is_none());
        let mut view = PermissionDoor::new(door_input(DoorMode::View)).unwrap();
        assert_eq!(view.message("set_code", b"1").unwrap(), Actions::default());
        assert!(view.message("close", b"").unwrap().events.is_empty());
    }

    // Catches treating a corrupt/missing decimal persistence record unlike source defaults.
    #[test]
    fn door_load_uses_source_u32_parse_and_zero_fallback() {
        for (bytes, expected) in [
            (None, "0"),
            (Some(b"bad".as_slice()), "0"),
            (Some(b"4294967295".as_slice()), "4294967295"),
        ] {
            let mut door = PermissionDoor::new(door_input(DoorMode::Edit)).unwrap();
            door.load(bytes).unwrap();
            assert_eq!(
                door.tick().ui[1],
                SourceUi::Text("door_code", expected.into())
            );
        }
    }

    // Catches turning checkpoint/rebind into a fresh initial state or losing load readiness.
    #[test]
    fn private_restore_preserves_loading_initialized_and_rebind_states() {
        let mut sign = Signs::new(sign_input(SignsMode::OwnerWrite)).unwrap();
        let mut pending = Signs::restore_private(&sign.save_private()).unwrap();
        assert_eq!(pending.rebind(), Actions::default());
        sign.load(Some(&[15, 0, 1, b'x'])).unwrap();
        let mut loaded = Signs::restore_private(&sign.save_private()).unwrap();
        assert_eq!(
            loaded.rebind().ui[1],
            SourceUi::Binary("signs_show", vec![15, 0, 1, b'x'])
        );
        loaded.message("set_message", &[15, 0, 1, b'z']).unwrap();
        let mut rebound = Signs::restore_private(&loaded.save_private()).unwrap();
        assert_eq!(
            rebound.rebind().ui[1],
            SourceUi::Binary("signs_show", vec![15, 0, 1, b'z'])
        );
        assert_eq!(rebound.tick(), Actions::default());

        let mut board = Scoreboard::new().unwrap();
        let mut empty = Scoreboard::restore_private(&board.save_private()).unwrap();
        assert_eq!(
            empty.rebind().ui,
            vec![SourceUi::Text("scoreboard_show", "".into())]
        );
        board.load(Some(&[4, 5, 1, 0, 2, 0])).unwrap();
        let mut loaded = Scoreboard::restore_private(&board.save_private()).unwrap();
        assert_eq!(
            loaded.rebind().ui[1],
            SourceUi::Binary("scoreboard_state", vec![4, 5, 1, 0, 2, 0])
        );

        let mut door = PermissionDoor::new(door_input(DoorMode::Edit)).unwrap();
        let mut pending = PermissionDoor::restore_private(&door.save_private()).unwrap();
        assert_eq!(pending.rebind(), Actions::default());
        door.load(Some(b"123")).unwrap();
        let mut loaded = PermissionDoor::restore_private(&door.save_private()).unwrap();
        assert_eq!(
            loaded.rebind().ui[1],
            SourceUi::Text("door_code", "123".into())
        );
        loaded.message("set_code", b"456").unwrap();
        let mut saved = PermissionDoor::restore_private(&loaded.save_private()).unwrap();
        assert_eq!(
            saved.rebind().ui[1],
            SourceUi::Text("door_code", "123".into())
        );
    }

    // Catches prefix-only checkpoint parsing and invalid readiness/authority state.
    #[test]
    fn private_checkpoint_decoders_reject_corruption_and_trailing_data() {
        let sign = Signs::new(sign_input(SignsMode::Write))
            .unwrap()
            .save_private();
        let board = Scoreboard::new().unwrap().save_private();
        let door = PermissionDoor::new(door_input(DoorMode::View))
            .unwrap()
            .save_private();
        for bytes in [&sign[..0], &sign[..sign.len() - 1], b"wrong"] {
            assert!(Signs::restore_private(bytes).is_err());
        }
        for bytes in [&board[..0], &board[..board.len() - 1], b"wrong"] {
            assert!(Scoreboard::restore_private(bytes).is_err());
        }
        for bytes in [&door[..0], &door[..door.len() - 1], b"wrong"] {
            assert!(PermissionDoor::restore_private(bytes).is_err());
        }
        let mut extra = sign;
        extra.push(0);
        assert!(Signs::restore_private(&extra).is_err());
        let mut extra = board;
        extra.push(0);
        assert!(Scoreboard::restore_private(&extra).is_err());
        let mut extra = door;
        extra.push(0);
        assert!(PermissionDoor::restore_private(&extra).is_err());
    }

    // Catches a checkpoint restoring one score while replaying a different score.
    #[test]
    fn scoreboard_persistence_match_rejects_independent_state_or_intent_changes() {
        let mut board = Scoreboard::new().unwrap();
        assert!(board.matches_persistence(None, false));
        assert!(!board.matches_persistence(None, true));
        board.load(Some(&[0, 1, 0, 0, 17, 0])).unwrap();
        assert!(board.matches_persistence(Some(&[0, 1, 0, 0, 17, 0]), false));
        assert!(board.matches_persistence(Some(&[0, 1, 0, 0, 17, 0, 88]), false));
        assert!(!board.matches_persistence(Some(&[0, 1, 0, 0, 18, 0]), false));
        assert!(board.matches_persistence(Some(&[0, 1, 0, 0, 17, 0]), true));
        assert!(!board.matches_persistence(Some(&[0, 1, 0, 0, 18, 0]), true));
        assert!(!board.matches_persistence(Some(&[0, 1, 0, 0, 17, 0, 88]), true));
        assert!(!board.matches_persistence(None, true));
    }

    // Initial read-denial redaction is a source state change, never a new write.
    #[test]
    fn signs_persistence_match_distinguishes_redaction_from_pending_writes() {
        let mut input = sign_input(SignsMode::Read);
        input.is_roommate = false;
        input.owner_authorized = false;
        let mut sign = Signs::new(input).unwrap();
        assert!(sign.matches_persistence(None, false));
        let original = &[0, 0, 6, b's', b'e', b'c', b'r', b'e', b't'];
        sign.load(Some(original)).unwrap();
        assert!(sign.matches_persistence(Some(original), false));
        assert!(!sign.matches_persistence(Some(original), true));
        sign.tick();
        assert!(sign.matches_persistence(Some(original), false));
        assert!(
            !sign.matches_persistence(Some(&[1, 0, 6, b's', b'e', b'c', b'r', b'e', b't']), false)
        );
        assert!(!sign.matches_persistence(Some(&[0, 0, 0]), true));

        let mut input = sign_input(SignsMode::OwnerWrite);
        input.max_length = 1;
        let mut owner = Signs::new(input).unwrap();
        owner.load(Some(&[15, 0, 2, b'x', b'x'])).unwrap();
        owner.tick();
        assert!(owner.matches_persistence(Some(&[15, 0, 2, b'x', b'x']), false));
        assert!(!owner.matches_persistence(Some(&[15, 0, 2, b'x', b'x']), true));
        owner.message("set_message", &[15, 0, 1, b'z']).unwrap();
        assert!(owner.matches_persistence(Some(&[15, 0, 1, b'z']), true));
        assert!(!owner.matches_persistence(Some(&[15, 0, 1, b'x']), true));
        assert!(!owner.matches_persistence(Some(&[15, 0, 1, b'z', 88]), true));
    }

    // An orphan intent must still be a byte sequence the handler can generate.
    #[test]
    fn canonical_writes_reject_source_tails_and_excess_sign_utf16_units() {
        assert!(Signs::canonical_write(&[15, 0, 1, b'x']));
        assert!(!Signs::canonical_write(&[15, 0, 1, b'x', 88]));
        assert!(!Signs::canonical_write(&[15, 0, 1, 255]));
        let mut long = vec![15, 0, 128, 128, 2];
        long.extend_from_slice(&[b'x'; 32768]);
        assert!(!Signs::canonical_write(&long));
        assert!(Scoreboard::canonical_write(&[254, 253, 255, 255, 0, 128]));
        assert!(!Scoreboard::canonical_write(&[0, 1, 0, 0, 0]));
        assert!(!Scoreboard::canonical_write(&[0, 1, 0, 0, 0, 0, 88]));
    }

    // Only Edit may retain the source's old cached code after its successful save.
    #[test]
    fn door_persistence_match_enforces_nonedit_cache_and_canonical_edit_intents() {
        let mut view = PermissionDoor::new(door_input(DoorMode::View)).unwrap();
        assert!(view.matches_persistence(None, false));
        assert!(!view.matches_persistence(None, true));
        view.load(Some(b"123")).unwrap();
        assert!(view.matches_persistence(Some(b" +00123 \0"), false));
        assert!(!view.matches_persistence(Some(b"124"), false));
        assert!(!view.matches_persistence(Some(b"123"), true));
        let mut code_input = PermissionDoor::new(door_input(DoorMode::CodeInput)).unwrap();
        code_input.load(None).unwrap();
        assert!(code_input.matches_persistence(None, false));
        assert!(code_input.matches_persistence(Some(b"bad"), false));
        assert!(!code_input.matches_persistence(Some(b"1"), false));
        assert!(!code_input.matches_persistence(Some(b"0"), true));

        let mut editor = PermissionDoor::new(door_input(DoorMode::Edit)).unwrap();
        editor.load(Some(b"123")).unwrap();
        assert!(editor.matches_persistence(Some(b"456"), true));
        assert!(editor.matches_persistence(Some(b"456"), false));
        assert!(!editor.matches_persistence(Some(b"0456"), true));
        assert!(!editor.matches_persistence(Some(b"1000000000"), true));
        assert!(!editor.matches_persistence(None, true));
    }

    // Catches mixing protocol-private data with the public source event registers.
    #[test]
    fn source_object_event_codes_and_registers_are_exact() {
        for (event, expected) in [
            (SourceObjectEvent::SignsWriting(true), (1, vec![1])),
            (
                SourceObjectEvent::ScoreboardScore {
                    team: 1,
                    score: 999,
                },
                (1, vec![999]),
            ),
            (
                SourceObjectEvent::ScoreboardScore { team: 2, score: 12 },
                (2, vec![12]),
            ),
            (
                SourceObjectEvent::ScoreboardColor { team: 1, color: 7 },
                (3, vec![7]),
            ),
            (
                SourceObjectEvent::ScoreboardColor { team: 2, color: 6 },
                (4, vec![6]),
            ),
            (SourceObjectEvent::DoorSave, (1, vec![])),
            (SourceObjectEvent::DoorState(2), (2, vec![2])),
            (SourceObjectEvent::DoorFee(100), (3, vec![100])),
            (SourceObjectEvent::DoorFlags(252), (4, vec![252])),
            (SourceObjectEvent::DoorValidation(false), (7, vec![0])),
        ] {
            assert_eq!(event.source_event(), expected);
        }
    }
}
