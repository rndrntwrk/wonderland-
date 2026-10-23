//! Native account creation maps eligible source outfits without inventing identity.
use serde::{Deserialize, Serialize};
use wonderland_contracts::authoring::{AppearanceContent, AppearanceSelection};
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Entry {
    pub key: String,
    pub part: String,
    pub gender: String,
    pub skins: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct CreationCatalog {
    pub release: String,
    pub available_slots: u8,
    pub entries: Vec<Entry>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreationSpec {
    pub name: String,
    pub description: String,
    pub gender: String,
    pub skin: String,
    pub head_key: String,
    pub body_key: String,
    pub release: String,
}
fn packed(key: &str) -> Option<String> {
    let (file, kind) = crate::source_identity::outfit_key(key)?;
    Some(((u64::from(file) << 32) | u64::from(kind)).to_string())
}
fn gender(key: &str) -> Option<&'static str> {
    match key {
        "vitaboy:gender:0" => Some("male"),
        "vitaboy:gender:1" => Some("female"),
        _ => None,
    }
}
fn skin(key: &str) -> Option<&str> {
    key.strip_prefix("vitaboy:skin:")
        .filter(|v| matches!(*v, "light" | "medium" | "dark"))
}
fn value(key: &Option<wonderland_contracts::authoring::ContentKey>) -> Result<&str, String> {
    key.as_ref()
        .map(|k| k.as_ref())
        .ok_or_else(|| "Choose an original appearance.".to_owned())
}
impl CreationCatalog {
    pub fn choices(&self, source: &AppearanceContent) -> AppearanceContent {
        let mut out = source.clone();
        for (part, rows) in [("head", &mut out.heads), ("body", &mut out.bodies)] {
            rows.retain_mut(|row| {
                let Some(value) = packed(row.key.as_ref()) else {
                    return false;
                };
                let Some(entry) = self
                    .entries
                    .iter()
                    .find(|e| e.part == part && e.key == value)
                else {
                    return false;
                };
                if row.genders.is_empty() {
                    let key = match entry.gender.as_str() {
                        "male" => "vitaboy:gender:0",
                        "female" => "vitaboy:gender:1",
                        _ => return false,
                    };
                    row.genders = vec![key.into()];
                } else {
                    row.genders
                        .retain(|key| gender(key.as_ref()) == Some(entry.gender.as_str()));
                }
                if row.skin_tones.is_empty() {
                    row.skin_tones = entry
                        .skins
                        .iter()
                        .filter(|v| matches!(v.as_str(), "light" | "medium" | "dark"))
                        .map(|v| format!("vitaboy:skin:{v}").into())
                        .collect();
                } else {
                    row.skin_tones.retain(|key| {
                        skin(key.as_ref()).is_some_and(|s| entry.skins.iter().any(|v| v == s))
                    });
                }
                !row.genders.is_empty() && !row.skin_tones.is_empty()
            });
        }
        out.genders.retain(|key| {
            out.heads.iter().any(|h| h.genders.contains(&key.key))
                && out.bodies.iter().any(|b| b.genders.contains(&key.key))
        });
        out.skin_tones.retain(|key| {
            out.heads.iter().any(|h| h.skin_tones.contains(&key.key))
                && out.bodies.iter().any(|b| b.skin_tones.contains(&key.key))
        });
        out
    }
    pub fn specification(
        &self,
        content: &AppearanceContent,
        selection: &AppearanceSelection,
        name: &str,
        description: &str,
    ) -> Result<CreationSpec, String> {
        if self.available_slots == 0 {
            return Err("This account already has three Sims.".into());
        }
        self.choices(content)
            .validate_selection(selection)
            .map_err(|e| e.to_string())?;
        if name.len() < 3
            || name.len() > 24
            || !name.as_bytes()[0].is_ascii_alphabetic()
            || !name.bytes().all(|b| b.is_ascii_alphabetic() || b == b' ')
        {
            return Err("Use a name of 3–24 letters and spaces, starting with a letter.".into());
        }
        if description.chars().count() > 499 {
            return Err("Keep the description to 499 characters.".into());
        }
        let selected_gender =
            gender(value(&selection.gender)?).ok_or("Choose a supported gender.")?;
        let selected_skin = skin(value(&selection.skin_tone)?).ok_or("Choose a skin tone.")?;
        let head = packed(value(&selection.head)?).ok_or("Choose an original head.")?;
        let body = packed(value(&selection.body)?).ok_or("Choose an original body.")?;
        for (part, key) in [("head", &head), ("body", &body)] {
            if !self.entries.iter().any(|e| {
                e.part == part
                    && &e.key == key
                    && e.gender == selected_gender
                    && e.skins.iter().any(|s| s == selected_skin)
            }) {
                return Err("This appearance is not eligible for creation.".into());
            }
        }
        Ok(CreationSpec {
            name: name.into(),
            description: description.into(),
            gender: selected_gender.into(),
            skin: selected_skin.into(),
            head_key: head,
            body_key: body,
            release: self.release.clone(),
        })
    }
}
