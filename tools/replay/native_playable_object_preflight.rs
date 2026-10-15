//! Whole original-object dependency intake for the two-user demo candidate.
//! Uses the existing decoder/import preflight; never replaces missing routines.
mod content_scope;
use content_scope::{Resource, inspect};
use serde_json::json;
use std::collections::BTreeMap;

const SOURCES: [(&str, &[u8], &str); 4] = [
    (
        "fso_decor_2-in-1_Racing_Banner.iff",
        include_bytes!(
            "../../TSOClient/FSO.Content.TSO/Content/Objects/fso_decor_2-in-1_Racing_Banner.iff"
        ),
        "260a9db1ed017eacf2604beaf92dbf0cafae6c651b1f0296605eab2429503ab7",
    ),
    (
        "fso_event_tally.iff",
        include_bytes!("../../TSOClient/FSO.Content.TSO/Content/Objects/fso_event_tally.iff"),
        "bf8466b91f6f7203e7dc0c43fd8c5810dbab902d2fc5b0421e972b11623eef6f",
    ),
    (
        "fso-summer-candy-border.iff",
        include_bytes!(
            "../../TSOClient/FSO.Content.TSO/Content/Objects/fso-summer-candy-border.iff"
        ),
        "f15697f5a39a517d615285d37b78d6292a6660018758391003a16ca0ca6d563f",
    ),
    (
        "painting_s1_jackpotPoster.iff",
        include_bytes!(
            "../../TSOClient/FSO.Content.TSO/Content/Objects/painting_s1_jackpotPoster.iff"
        ),
        "311cfac5c10c02ede7442649a9cc6850a2b87cba72640b85de4953b13827ba95",
    ),
];

fn report() -> Result<serde_json::Value, String> {
    let mut sources = Vec::new();
    for (name, bytes, digest) in SOURCES {
        let resource = Resource::decode(name, bytes, digest)?;
        let mut counts = BTreeMap::<String, usize>::new();
        for chunk in &resource.file.chunks {
            *counts
                .entry(String::from_utf8_lossy(&chunk.key.kind).to_string())
                .or_default() += 1;
        }
        let audit = match inspect(&resource, None, None) {
            Ok(audit) => json!({"status":"inspected", "audit":audit}),
            Err(error) => json!({"status":"rejected-source-contract", "error":error}),
        };
        sources.push(json!({
            "source_path":format!("TSOClient/FSO.Content.TSO/Content/Objects/{name}"),
            "source_sha256":digest,
            "source_bytes":bytes.len(),
            "original_chunk_counts":counts,
            "result":audit,
        }));
    }
    Ok(json!({
        "schema":1,
        "source_baseline":"4c6b3e8f5835b228723caea3c9f683c62f244f73",
        "scope":"whole-unchanged-source-object-preflight-for-two-user-demo",
        "preferred_candidate":"fso_decor_2-in-1_Racing_Banner.iff",
        "preferred_guid":"6378e9e6",
        "essential_missing_base":"installed TSO objectdata/globals/global.iff",
        "immediate_missing_bhav_ids":[271,280],
        "menu": {
            "table":128,
            "original_theme_one_action":4102,
            "original_theme_one_check":4103,
            "original_theme_two_action":4104,
            "original_theme_two_check":4105,
            "routing_claim":false,
        },
        "whole_object_runtime_qualified":false,
        "playable_release_qualified":false,
        "runtime_metadata_supplied":false,
        "source_mutations":0,
        "missing_routine_substitutions":0,
        "sources":sources,
    }))
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !args.is_empty() && args != ["--require-playable"] {
        eprintln!("Only --require-playable is supported");
        std::process::exit(2);
    }
    match report() {
        Ok(value) => {
            println!("{}", serde_json::to_string_pretty(&value).unwrap());
            if !args.is_empty() {
                // This bounded probe cannot establish placed runtime acceptance.
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_keeps_original_definition_menu_visuals_and_both_missing_globals() {
        let (name, bytes, digest) = SOURCES[0];
        let resource = Resource::decode(name, bytes, digest).unwrap();
        let audit = inspect(&resource, None, None).unwrap();
        assert!(!audit.static_complete);
        assert!(!audit.whole_object_runtime_qualified);
        assert_eq!(audit.objects.len(), 1);
        assert_eq!(audit.objects[0].guid, 0x6378e9e6);
        assert_eq!(audit.semiglobal_required, None);
        assert!(audit.missing.iter().any(|s| s == "global:271"));
        assert!(audit.missing.iter().any(|s| s == "global:280"));
        for root in [4102, 4103, 4104, 4105] {
            assert!(audit.declared_roots.contains(&format!("private:{root}")));
        }
        for kind in [*b"DGRP", *b"SPR2", *b"PALT", *b"FSOM", *b"MTEX"] {
            assert!(resource.file.chunks.iter().any(|c| c.key.kind == kind));
        }
    }

    #[test]
    fn routing_border_and_zero_direct_call_poster_do_not_escape_source_closure() {
        for index in [2, 3] {
            let (name, bytes, digest) = SOURCES[index];
            let resource = Resource::decode(name, bytes, digest).unwrap();
            let audit = inspect(&resource, None, None).unwrap();
            assert!(!audit.static_complete);
            assert!(audit.missing.iter().any(|s| s == "global:265"));
            if index == 3 {
                assert_eq!(audit.semiglobal_required.as_deref(), Some("ArtGlobals"));
                for id in [8199, 8201, 8203] {
                    assert!(
                        audit
                            .missing
                            .iter()
                            .any(|s| s == &format!("semiglobal:{id}"))
                    );
                }
            }
        }
    }

    #[test]
    fn source_mutation_is_rejected_and_report_never_claims_playable_qualification() {
        let (name, bytes, digest) = SOURCES[0];
        let mut changed = bytes.to_vec();
        changed[100] ^= 1;
        assert!(Resource::decode(name, &changed, digest).is_err());
        let value = report().unwrap();
        assert_eq!(value["playable_release_qualified"], false);
        assert_eq!(value["source_mutations"], 0);
        assert_eq!(value["missing_routine_substitutions"], 0);
    }
}
