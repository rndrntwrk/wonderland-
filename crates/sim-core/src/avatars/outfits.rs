//! Outfit identity and source suit selectors; resource decoding belongs to B.
use super::AvatarPlatform;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutfitReference {
    Id(u64),
    Name(String),
    /// TS1 inline definitions are retained for snapshots instead of losing the
    /// source's transient OftData. B resolves the definition into appearances.
    Legacy {
        definition: String,
        head: bool,
        hand_group: Option<String>,
    },
}
impl OutfitReference {
    pub fn legacy_id(&self) -> u64 {
        match self {
            Self::Id(id) => *id,
            Self::Name(_) => u32::MAX as u64,
            Self::Legacy { .. } => 0,
        }
    }
    pub fn parse_source(input: &str, _platform: AvatarPlatform) -> Result<Self, OutfitError> {
        let value = input.trim();
        let value = value
            .strip_prefix("0x")
            .or_else(|| value.strip_prefix("0X"))
            .unwrap_or(value);
        u64::from_str_radix(value, 16)
            .map(Self::Id)
            .map_err(|_| OutfitError::InvalidReference)
    }
    pub fn validate(&self) -> Result<(), OutfitError> {
        match self {
            Self::Name(name) if name.len() > 16_384 => Err(OutfitError::InvalidReference),
            Self::Legacy {
                definition,
                hand_group,
                ..
            } if definition.len() > 16_384
                || hand_group.as_ref().is_some_and(|s| s.len() > 16_384) =>
            {
                Err(OutfitError::InvalidReference)
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefaultSuits {
    pub daywear: OutfitReference,
    pub swimwear: OutfitReference,
    pub sleepwear: OutfitReference,
}
impl DefaultSuits {
    pub fn new(female: bool) -> Self {
        Self {
            daywear: OutfitReference::Id(0x24c0000000d),
            swimwear: OutfitReference::Id(if female { 0x620000000d } else { 0x5470000000d }),
            sleepwear: OutfitReference::Id(if female { 0x5150000000d } else { 0x5440000000d }),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DynamicSuits {
    pub daywear: u64,
    pub swimwear: u64,
    pub sleepwear: u64,
    pub costume: u64,
}
impl DynamicSuits {
    pub fn new(female: bool) -> Self {
        let default = DefaultSuits::new(female);
        Self {
            daywear: default.daywear.legacy_id(),
            swimwear: default.swimwear.legacy_id(),
            sleepwear: default.sleepwear.legacy_id(),
            costume: default.daywear.legacy_id(),
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decorations {
    pub head: u64,
    pub back: u64,
    pub shoes: u64,
    pub tail: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutfitState {
    pub body: Option<OutfitReference>,
    pub head: Option<OutfitReference>,
    pub defaults: DefaultSuits,
    pub dynamic: DynamicSuits,
    pub decorations: Decorations,
    pub skin_tone: u8,
}
impl Default for OutfitState {
    fn default() -> Self {
        Self {
            body: None,
            head: None,
            defaults: DefaultSuits::new(false),
            dynamic: DynamicSuits::new(false),
            decorations: Decorations::default(),
            skin_tone: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutfitError {
    InvalidReference,
    UnsupportedSuit(u16),
    MissingLegacyBodyString(u16),
    MissingLegacyJobUniform,
}

impl OutfitState {
    pub fn validate(&self) -> Result<(), OutfitError> {
        for reference in [
            &self.defaults.daywear,
            &self.defaults.swimwear,
            &self.defaults.sleepwear,
        ]
        .into_iter()
        .chain(self.body.iter())
        .chain(self.head.iter())
        {
            reference.validate()?;
        }
        Ok(())
    }
    /// VMNetSetOutfitCmd updates suit storage, not the currently worn body.
    pub fn set_stored(&mut self, scope: u16, outfit: u64) -> bool {
        match scope {
            0 => self.defaults.daywear = OutfitReference::Id(outfit),
            2 => self.defaults.swimwear = OutfitReference::Id(outfit),
            5 => self.defaults.sleepwear = OutfitReference::Id(outfit),
            8 => self.decorations.head = outfit,
            9 => self.decorations.back = outfit,
            10 => self.decorations.shoes = outfit,
            11 => self.decorations.tail = outfit,
            22 => self.dynamic.daywear = outfit,
            23 => self.dynamic.swimwear = outfit,
            24 => self.dynamic.sleepwear = outfit,
            25 => self.dynamic.costume = outfit,
            _ => return false,
        }
        true
    }
    pub fn resolve_tso(
        &self,
        scope: u16,
        gender: i16,
        job_id: i16,
        job_grade: i16,
    ) -> Result<OutfitReference, OutfitError> {
        let male = gender == 0;
        let value = match scope {
            0 => self.defaults.daywear.legacy_id(),
            1 => {
                if male {
                    0x24e0000000d
                } else {
                    0x10000000d
                }
            }
            2 => self.defaults.swimwear.legacy_id(),
            3 => {
                if !(1..=5).contains(&job_id) {
                    return Err(OutfitError::UnsupportedSuit(scope));
                }
                let level = ((i32::from(job_grade) + 1) / 4).clamp(0, 2) as usize;
                JOB_OUTFITS[usize::from(!male)][job_id as usize - 1][level]
            }
            5 => self.defaults.sleepwear.legacy_id(),
            6 => 0x5750000000d,
            7 => 0x5740000000d,
            8 => self.decorations.head,
            9 => self.decorations.back,
            10 => self.decorations.shoes,
            11 => self.decorations.tail,
            20 => {
                if male {
                    0x2900000000d
                } else {
                    0x4a0000000d
                }
            }
            22 => self.dynamic.daywear,
            23 => self.dynamic.swimwear,
            24 => self.dynamic.sleepwear,
            25 => self.dynamic.costume,
            128 => 0,
            _ => return Err(OutfitError::UnsupportedSuit(scope)),
        };
        Ok(OutfitReference::Id(value))
    }
    /// Off-lot avatar persistence resets borrowed rack outfits and naked bodies
    /// to daywear. An in-lot snapshot retains the actual body without this rule.
    pub fn body_for_persistence(&self) -> Option<OutfitReference> {
        let body = self.body.as_ref()?;
        let id = body.legacy_id();
        if [
            self.dynamic.daywear,
            self.dynamic.sleepwear,
            self.dynamic.swimwear,
            0x24e0000000d,
            0x10000000d,
        ]
        .contains(&id)
        {
            Some(self.defaults.daywear.clone())
        } else {
            Some(body.clone())
        }
    }
}

const JOB_OUTFITS: [[[u64; 3]; 5]; 2] = [
    [
        [0x5870000000d, 0x5880000000d, 0x5890000000d],
        [0x58b0000000d, 0x58d0000000d, 0x58e0000000d],
        [0x58a0000000d, 0x58c0000000d, 0x58f0000000d],
        [0x5900000000d, 0x5940000000d, 0x5920000000d],
        [0x5910000000d, 0x5950000000d, 0x5930000000d],
    ],
    [
        [0x5780000000d, 0x57a0000000d, 0x5790000000d],
        [0x57c0000000d, 0x57e0000000d, 0x57f0000000d],
        [0x57b0000000d, 0x57d0000000d, 0x5800000000d],
        [0x5810000000d, 0x5850000000d, 0x5830000000d],
        [0x5820000000d, 0x5860000000d, 0x5840000000d],
    ],
];

/// Only semantic strings supplied by the content provider, including a resolved
/// job uniform. This module never discovers files or decodes legacy resources.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacySuitInputs {
    pub body_strings: Vec<String>,
    pub gender: i16,
    pub age: i16,
    pub hand_group: Option<String>,
    pub job_uniform: Option<(String, String)>,
}
impl LegacySuitInputs {
    fn string(&self, index: usize) -> Result<&str, OutfitError> {
        self.body_strings
            .get(index)
            .map(String::as_str)
            .ok_or(OutfitError::MissingLegacyBodyString(index as u16))
    }
    pub fn resolve(&self, scope: u16) -> Result<OutfitReference, OutfitError> {
        self.resolve_inner(scope, true)
    }

    /// Resolve a job mesh after the source provider's male-mesh fallback and
    /// gender override have already run. An explicitly empty female override
    /// remains a selected mesh; it must not repeat the male empty-mesh fallback.
    pub fn resolve_selected_job_uniform(&self) -> Result<OutfitReference, OutfitError> {
        self.resolve_inner(3, false)
    }

    fn resolve_inner(
        &self,
        scope: u16,
        empty_uniform_returns_daywear: bool,
    ) -> Result<OutfitReference, OutfitError> {
        let male = self.gender == 0;
        let child = self.age < 18 && self.age != 0;
        let body = self.string(1)?.to_lowercase();
        let skin = self.string(14)?.to_lowercase();
        let shape = if body.contains("fat") {
            "fat"
        } else if body.contains("skn") {
            "skn"
        } else {
            "fit"
        };
        let code = if child {
            "uchd".to_owned()
        } else {
            format!("{}{shape}", if male { "m" } else { "f" })
        };
        let gen = if male { "m" } else { "f" };
        let valid = |s: &str| !s.is_empty() && s != "ADDED";
        let mut head = false;
        let definition = match scope {
            0 => self.string(1)?.to_owned(),
            1 => format!("n{code}_01,BODY=n{code}{skin}_01"),
            2 => {
                if self.body_strings.get(31).is_some_and(|s| valid(s)) {
                    self.string(31)?.into()
                } else {
                    format!(
                        "n{code}_01,BODY=u{code}{skin}_{}01",
                        if male && !child { "briefs" } else { "undies" }
                    )
                }
            }
            3 => {
                let (mesh, texture) = self
                    .job_uniform
                    .as_ref()
                    .ok_or(OutfitError::MissingLegacyJobUniform)?;
                if mesh.is_empty() && empty_uniform_returns_daywear {
                    self.string(1)?.into()
                } else {
                    let substitute = |s: &str| {
                        s.to_lowercase()
                            .replace("$g", &code[..1])
                            .replace("$b", &code[1..])
                            .replace("$c", &skin)
                    };
                    format!("{},BODY={}", substitute(mesh), substitute(texture))
                }
            }
            4 => {
                if self.body_strings.get(30).is_some_and(|s| valid(s)) {
                    self.string(30)?.into()
                } else {
                    format!("f{code}_01,BODY=f{code}{skin}_01")
                }
            }
            5 => {
                if self.body_strings.get(32).is_some_and(|s| valid(s)) {
                    self.string(32)?.into()
                } else {
                    let prefix = if child { "pjs" } else { "pajama" };
                    let bodygen = if child {
                        format!("{gen}chd")
                    } else {
                        code.clone()
                    };
                    format!("{prefix}{bodygen}_01,BODY={prefix}{gen}{skin}_01")
                }
            }
            6 | 7 => {
                head = true;
                format!(
                    "{}_01,BODY={}_01",
                    if child { "skeletonchd" } else { "skeleton" },
                    if scope == 6 { "skeleton" } else { "skeleneg" }
                )
            }
            8..=13 => self.string(30)?.into(),
            14 => self.string(31)?.into(),
            15 => self.string(32)?.into(),
            16 | 17 => self.string(33)?.into(),
            18 => self.string(34)?.into(),
            _ => return Err(OutfitError::UnsupportedSuit(scope)),
        };
        Ok(OutfitReference::Legacy {
            definition,
            head,
            hand_group: self.hand_group.clone(),
        })
    }
}
