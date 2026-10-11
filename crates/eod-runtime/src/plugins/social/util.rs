// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
use super::*;

/// A retained link must resolve to the original native cluster, plugin, and
/// living group. Detached controllers still exist in this list during restore;
/// attachment is deliberately a host readiness check, not a reference check.
pub(super) fn linked_peer(
    peers: &[PeerGroup],
    group: InstanceId,
    cluster: u64,
    plugin: PluginId,
) -> Option<&PeerGroup> {
    peers.iter().find(|peer| {
        peer.group == group
            && peer.cluster == cluster
            && peer.plugin == plugin
            && peer.object != 0
            && !peer.closing
    })
}

#[derive(Clone)]
pub(super) struct Rng {
    state: u64,
    draws: u64,
}
impl Rng {
    pub(super) fn new(seed: u64) -> Self {
        Self {
            state: seed,
            draws: 0,
        }
    }
    pub(super) fn below(&mut self, bound: u32) -> Result<u32, Error> {
        if bound == 0 {
            return Err(Error::InvalidPluginInput);
        }
        let bound = u64::from(bound);
        let threshold = bound.wrapping_neg() % bound;
        for _ in 0..32 {
            self.draws = self
                .draws
                .checked_add(1)
                .filter(|n| *n != u64::MAX)
                .ok_or(Error::CounterExhausted)?;
            self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
            let mut x = self.state;
            x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
            x ^= x >> 31;
            if x >= threshold {
                return Ok((x % bound) as u32);
            }
        }
        Err(Error::InvalidPluginInput)
    }
    pub(super) fn save(&self, w: &mut Writer) {
        w.u64(self.state);
        w.u64(self.draws);
    }
    pub(super) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let state = r.u64()?;
        let draws = r.u64()?;
        if draws == u64::MAX {
            return Err(Error::InvalidCheckpoint);
        }
        Ok(Self { state, draws })
    }
}

pub(super) fn strings<'a>(values: impl IntoIterator<Item = &'a str>) -> Vec<u8> {
    let mut result = Vec::new();
    for value in values {
        let mut len = value.len();
        while len >= 128 {
            result.push((len as u8) | 0x80);
            len >>= 7;
        }
        result.push(len as u8);
        result.extend_from_slice(value.as_bytes());
    }
    result
}
pub(super) fn by_role(roster: &Roster, role: u8) -> Option<Member> {
    roster
        .iter()
        .flatten()
        .copied()
        .find(|member| member.input.role == role)
}
pub(super) fn valid_roles(roster: &Roster, count: u8) -> bool {
    let mut seen = [false; 16];
    for member in roster.iter().flatten() {
        let role = member.input.role;
        if role >= count
            || seen[usize::from(role)]
            || member.avatar_object <= 0
            || member.avatar_id == 0
        {
            return false;
        }
        seen[usize::from(role)] = true;
    }
    true
}
pub(super) fn full(roster: &Roster, count: u8) -> bool {
    (0..count).all(|role| by_role(roster, role).is_some())
}
pub(super) fn role_binary(
    a: &mut Actions,
    roster: &Roster,
    role: u8,
    event: &'static str,
    bytes: Vec<u8>,
) {
    if let Some(member) = by_role(roster, role) {
        a.binary(Target::Member(member.seat), event, bytes);
    }
}
pub(super) fn role_text(
    a: &mut Actions,
    roster: &Roster,
    role: u8,
    event: &'static str,
    text: String,
) {
    if let Some(member) = by_role(roster, role) {
        a.text(Target::Member(member.seat), event, text);
    }
}
pub(super) fn first_member(roster: &Roster) -> Option<Member> {
    roster.iter().flatten().next().copied()
}
pub(super) fn name_for(names: &[AvatarName], member: Member) -> Result<&str, Error> {
    names
        .iter()
        .find(|entry| entry.avatar_id == member.avatar_id)
        .map(|entry| entry.name.as_str())
        .ok_or(Error::InvalidPluginInput)
}
pub(super) fn validate_names(names: &[AvatarName]) -> bool {
    names.len() <= 16
        && names.iter().enumerate().all(|(i, name)| {
            name.avatar_id != 0
                && name.name.len() <= 128
                && !name.name.contains('\0')
                && !names[..i]
                    .iter()
                    .any(|other| other.avatar_id == name.avatar_id)
        })
}
pub(super) fn save_names(names: &[AvatarName], w: &mut Writer) {
    w.u8(names.len() as u8);
    for name in names {
        w.u32(name.avatar_id);
        w.string(&name.name);
    }
}
pub(super) fn restore_names(r: &mut Reader<'_>) -> Result<Vec<AvatarName>, Error> {
    let len = r.u8()?;
    if len > 16 {
        return Err(Error::InvalidCheckpoint);
    }
    let mut names = Vec::with_capacity(usize::from(len));
    for _ in 0..len {
        names.push(AvatarName {
            avatar_id: r.u32()?,
            name: r.string(128)?,
        });
    }
    if !validate_names(&names) {
        return Err(Error::InvalidCheckpoint);
    }
    Ok(names)
}
pub(super) fn save_option_id(value: Option<InstanceId>, w: &mut Writer) {
    w.bool(value.is_some());
    if let Some(value) = value {
        w.u64(value.0);
    }
}
pub(super) fn restore_option_id(r: &mut Reader<'_>) -> Result<Option<InstanceId>, Error> {
    if r.bool()? {
        let value = r.u64()?;
        if value == 0 {
            return Err(Error::InvalidCheckpoint);
        }
        Ok(Some(InstanceId(value)))
    } else {
        Ok(None)
    }
}
pub(super) fn signal(code: u16, numbers: Vec<i64>, bytes: Vec<u8>) -> Signal {
    Signal {
        code,
        numbers,
        bytes,
    }
}
pub(super) fn target_id(id: InstanceId) -> i64 {
    i64::from_ne_bytes(id.0.to_ne_bytes())
}
pub(super) fn decode_id(value: i64) -> InstanceId {
    InstanceId(u64::from_ne_bytes(value.to_ne_bytes()))
}
pub(super) fn append(a: &mut Actions, b: Actions) {
    a.items.extend(b.items);
}
pub(super) fn i32_payload(bytes: &[u8]) -> Result<i32, Error> {
    let bytes: [u8; 4] = bytes
        .get(..4)
        .ok_or(Error::InvalidMessage)?
        .try_into()
        .map_err(|_| Error::InvalidMessage)?;
    Ok(i32::from_le_bytes(bytes))
}
pub(super) fn short_payload(bytes: &[u8]) -> Option<i16> {
    Some(i16::from_le_bytes(bytes.get(..2)?.try_into().ok()?))
}
pub(super) fn valid_single(roster: &Roster) -> bool {
    valid_roles(roster, 1) && roster.iter().flatten().count() <= 1
}
