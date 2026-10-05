// Source-derived from FreeSO STR.cs, subject to the Mozilla Public License,
// v. 2.0. https://mozilla.org/MPL/2.0/
//! Localization by source language-set rules.
use wonderland_legacy_formats::semantic::{StringItem, Strings};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LocaleSelection {
    pub requested: u8,
    pub default_language: u8,
}

impl Default for LocaleSelection {
    fn default() -> Self {
        Self {
            requested: 0,
            default_language: 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalizedString<'a> {
    pub item: &'a StringItem,
    /// One-based set number used for lookup, independent of the item's raw code.
    pub language: u8,
    pub used_fallback: bool,
}

pub fn lookup_string(
    table: &Strings,
    index: usize,
    locale: LocaleSelection,
) -> Option<LocalizedString<'_>> {
    let requested = if locale.requested == 0 {
        locale.default_language
    } else {
        locale.requested
    };
    let selected = requested
        .checked_sub(1)
        .and_then(|i| table.sets.get(usize::from(i)));
    let (set, language, used_fallback) = if let Some(set) = selected.filter(|set| !set.is_empty()) {
        (set, requested, false)
    } else {
        (table.sets.first()?, 1, requested != 1)
    };
    set.get(index).map(|item| LocalizedString {
        item,
        language,
        used_fallback,
    })
}
