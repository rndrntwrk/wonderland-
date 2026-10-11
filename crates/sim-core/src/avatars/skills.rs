use super::state::{AvatarPermissions, PersonData};
use super::AvatarPlatform;
use crate::ids::PersistentId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Skill {
    Body = 0,
    Charisma = 1,
    Cooking = 2,
    Creativity = 3,
    Logic = 4,
    Mechanical = 5,
}
pub const SKILLS: [Skill; 6] = [
    Skill::Body,
    Skill::Charisma,
    Skill::Cooking,
    Skill::Creativity,
    Skill::Logic,
    Skill::Mechanical,
];
impl Skill {
    pub fn person_index(self) -> usize {
        [17, 11, 10, 15, 18, 12][self as usize]
    }
    pub fn lock_index(self) -> usize {
        81 + self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillPolicy {
    pub platform: AvatarPlatform,
    pub persistent_id: PersistentId,
    pub permissions: AvatarPermissions,
    pub force_enable: bool,
    pub new_player: bool,
    pub lot_category: u8,
    pub skill_mode: u8,
    pub has_thread: bool,
}
impl SkillPolicy {
    pub fn multiplier(&self) -> i32 {
        if !self.has_thread
            || self.force_enable
            || self.persistent_id.0 == 0
            || self.platform == AvatarPlatform::Ts1
        {
            return 1;
        }
        if self.lot_category == 7 && self.new_player {
            return 2;
        }
        match self.skill_mode {
            0 => 1,
            1 => i32::from(self.permissions != AvatarPermissions::Visitor),
            _ => 0,
        }
    }
    /// VMAvatar.GetPersonData(70) is a gameplay disable mask, whereas raw slot
    /// 70 is a count of available user skill locks (or TS1 zodiac).
    pub fn script_lock_mask(&self) -> i16 {
        if self.has_thread && self.multiplier() == 0 {
            0x7fff
        } else {
            0
        }
    }
    pub fn write(&self, data: &mut PersonData, skill: Skill, requested: i16) -> SkillWrite {
        let old = data.values[skill.person_index()];
        let mul = self.multiplier();
        if mul == 0 {
            return SkillWrite {
                old,
                value: old,
                blocked: true,
            };
        }
        let delta = i32::from(requested) - i32::from(old);
        let value = if mul != 1 && delta > 0 {
            (i32::from(old) + delta * mul) as i16
        } else {
            requested
        };
        data.values[skill.person_index()] = value;
        SkillWrite {
            old,
            value,
            blocked: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillWrite {
    pub old: i16,
    pub value: i16,
    pub blocked: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkillLockResult {
    Applied { skill: Skill, level: i16 },
    NoCapacity,
}

/// VMNetSkillLockCmd clamps the skill index to five and does not clamp negative
/// levels. Higher-level command validation can reject a negative request before
/// invoking this compatibility provider; this function preserves the source.
pub fn set_skill_lock(data: &mut PersonData, skill_id: u8, requested: i16) -> SkillLockResult {
    let skill = SKILLS[usize::from(skill_id.min(5))];
    let limit = data.values[70];
    let others: i32 = SKILLS
        .into_iter()
        .filter(|s| *s != skill)
        .map(|s| i32::from(data.values[s.lock_index()] / 100))
        .sum();
    if others >= i32::from(limit) {
        return SkillLockResult::NoCapacity;
    }
    let level = (data.values[skill.person_index()] / 100)
        .min(requested.min((i32::from(limit) - others) as i16));
    data.values[skill.lock_index()] = level.wrapping_mul(100);
    SkillLockResult::Applied { skill, level }
}
