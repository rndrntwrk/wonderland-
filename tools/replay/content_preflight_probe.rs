//! Fixed, hash-pinned whole-resource intake. No artwork is emitted, no VM starts,
//! and no missing provider is filled with an empty or guessed global resource.
mod content_scope;
use content_scope::{Resource, hash, inspect};
use serde_json::json;
#[cfg(target_arch = "wasm32")]
use std::sync::OnceLock;

const COHORT: [(&str, &[u8], &str); 4] = [
    (
        "Casino_2-Tile_Bar_CC.iff",
        include_bytes!("../../TSOClient/FSO.Content.TSO/Content/Objects/Casino_2-Tile_Bar_CC.iff"),
        "20b67e06940bfe0a89b689312fe001721ff3760cce10fca104373bf1122c4865",
    ),
    (
        "Chair_fso_Bouncy_Beach_Ball.iff",
        include_bytes!(
            "../../TSOClient/FSO.Content.TSO/Content/Objects/Chair_fso_Bouncy_Beach_Ball.iff"
        ),
        "ab9975d947e64ea19ac194679dee6080f99a5b4c0a4c89f03f719d75113d57a7",
    ),
    (
        "fso_christmas_flag.iff",
        include_bytes!("../../TSOClient/FSO.Content.TSO/Content/Objects/fso_christmas_flag.iff"),
        "d12dcf5254b1381c7ad8f4549c6365c4d2f31f1170a5fd96a5fdf037a195f188",
    ),
    (
        "cursebook_set_permission.iff",
        include_bytes!(
            "../../TSOClient/FSO.Content.TSO/Content/Objects/cursebook_set_permission.iff"
        ),
        "fab1a27e78532339eb42ebceda39eaa2a17cff9a4c16226e66ec2a1048938f91",
    ),
];
#[cfg(target_arch = "wasm32")]
static OUTPUT: OnceLock<(String, bool)> = OnceLock::new();
fn run() -> Result<(String, bool), String> {
    let mut resources = Vec::new();
    let mut incomplete = 0;
    for (index, (name, bytes, digest)) in COHORT.iter().enumerate() {
        let bad = cfg!(preflight_fault_pin) && index == 0;
        let resource = Resource::decode(
            name,
            bytes,
            if bad {
                "0000000000000000000000000000000000000000000000000000000000000000"
            } else {
                digest
            },
        )?;
        match inspect(&resource, None, None) {
            Ok(audit) => {
                if !audit.static_complete {
                    incomplete += 1;
                }
                resources.push(json!({"source":name,"source_sha256":digest,"source_bytes":bytes.len(),"status":"inspected","audit":audit}));
            }
            Err(reason) => {
                incomplete += 1;
                resources.push(json!({"source":name,"source_sha256":hash(bytes),"source_bytes":bytes.len(),"status":"rejected-source-contract","static_complete":false,"error":reason}));
            }
        }
    }
    let report = json!({"schema":1,"audit_completed":true,"scope":"pinned-whole-resource-static-intake", "resource_count":COHORT.len(),"incomplete_resources":incomplete,
        "static_cohort_complete":incomplete==0,"whole_object_runtime_qualified":false,
        "original_vm_executed":false,"placed_world_executed":false,
        "resource_mutations":0,"empty_dependency_substitutions":0,
        "resources":resources});
    Ok((
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())? + "\n",
        incomplete == 0,
    ))
}
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !args.is_empty() && args != ["--require-static-closure"] {
        eprintln!("Only --require-static-closure is supported");
        std::process::exit(2);
    }
    match run() {
        Ok((text, ready)) => {
            print!("{text}");
            if !args.is_empty() && !ready {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
#[cfg(target_arch = "wasm32")]
fn output() -> &'static (String, bool) {
    OUTPUT.get_or_init(|| run().expect("pinned native content source must remain valid"))
}
// SAFETY: immutable process-lifetime probe output; no caller-supplied pointer,
// mutation or deallocation API is provided. These exports are probe-only.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn reference_ptr() -> *const u8 {
    output().0.as_ptr()
}
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn reference_len() -> usize {
    output().0.len()
}
