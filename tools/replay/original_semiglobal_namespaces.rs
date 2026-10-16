//! Explicit deterministic importer identities for distinct effective original
//! semiglobals. These are not original GUIDs. Sealed scope resources bind each
//! chosen owner to its effective source bytes and patch/tuning provenance.
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use wonderland_asset_cooker::manifest::CookLimits;
use wonderland_content_ir::{objects::ResolvedContent, patches::iff_sha256};
use wonderland_legacy_formats::semantic::decode_objd;

fn assign(
    object_guids: &BTreeSet<u32>,
    identities: &[Option<(String, [u8; 32], [u8; 32])>],
) -> Result<Vec<Option<u32>>, String> {
    let mut namespaces = BTreeMap::new();
    let mut next = 1u32;
    identities
        .iter()
        .map(|identity| {
            let Some(identity) = identity else {
                return Ok(None);
            };
            if let Some(owner) = namespaces.get(identity) {
                return Ok(Some(*owner));
            }
            while object_guids.contains(&next) {
                next = next
                    .checked_add(1)
                    .ok_or("semiglobal importer namespace exhausted")?;
            }
            let owner = next;
            next = next
                .checked_add(1)
                .ok_or("semiglobal importer namespace exhausted")?;
            namespaces.insert(identity.clone(), owner);
            Ok(Some(owner))
        })
        .collect()
}

/// Source order is explicit in the cook spec, and repeated effective scopes
/// share an owner. Name, byte or effective tuning differences never alias a
/// previous scope. Scope resources independently seal original provenance.
pub fn owners(
    contents: &[ResolvedContent],
    limits: &CookLimits,
) -> Result<Vec<Option<u32>>, String> {
    let mut guids = BTreeSet::new();
    let mut identities = Vec::with_capacity(contents.len());
    for content in contents {
        for chunk in content
            .iff
            .chunks
            .iter()
            .filter(|chunk| chunk.key.kind == *b"OBJD")
        {
            guids.insert(
                decode_objd(&chunk.data, &limits.legacy)
                    .map_err(|error| error.to_string())?
                    .guid(),
            );
        }
        identities.push(
            content
                .semiglobal
                .as_ref()
                .map(|scope| {
                    let hash = iff_sha256(&scope.iff, &limits.legacy)
                        .map_err(|error| error.to_string())?;
                    let tuning = serde_json::to_vec(&content.tuning.semiglobal_cache)
                        .map_err(|error| error.to_string())?;
                    Ok::<_, String>((scope.name.clone(), hash, Sha256::digest(tuning).into()))
                })
                .transpose()?,
        );
    }
    assign(&guids, &identities)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn effective_scopes_are_distinct_repeatable_and_cannot_alias_object_guids() {
        let person = Some(("personglobals.iff".into(), [1; 32], [9; 32]));
        let mail = Some(("mailsemiglobal.iff".into(), [2; 32], [9; 32]));
        let changed_mail = Some(("mailsemiglobal.iff".into(), [3; 32], [9; 32]));
        let inputs = [
            person.clone(),
            None,
            mail.clone(),
            person,
            changed_mail,
            mail,
        ];
        let expected = vec![Some(2), None, Some(4), Some(2), Some(5), Some(4)];
        assert_eq!(assign(&BTreeSet::from([1, 3]), &inputs).unwrap(), expected);
        assert_eq!(assign(&BTreeSet::from([1, 3]), &inputs).unwrap(), expected);
    }
    #[test]
    fn effective_tuning_changes_cannot_alias() {
        let scopes = [
            Some(("same.iff".into(), [1; 32], [2; 32])),
            Some(("same.iff".into(), [1; 32], [3; 32])),
        ];
        assert_eq!(
            assign(&BTreeSet::new(), &scopes).unwrap(),
            vec![Some(1), Some(2)]
        );
    }
    #[test]
    fn existing_single_person_scope_keeps_owner_one() {
        let scopes = [
            Some(("personglobals.iff".into(), [1; 32], [9; 32])),
            None,
            None,
        ];
        assert_eq!(
            assign(&BTreeSet::from([0x7fd96b54, 0x6378e9e6]), &scopes).unwrap(),
            vec![Some(1), None, None]
        );
    }
}
