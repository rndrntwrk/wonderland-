//! Content conversion and isolated runtime tools; see docs/swarm-b/runtime-bridge.md.
#![forbid(unsafe_code)]

mod budget;

pub use sim_core;

pub const SIM_CORE_REVISION: &str = "8a0e251d19e222a0a6833d7408ca629f674e1729";

pub fn import_bhav(
    chunk: &wonderland_legacy_formats::iff::IffChunk,
    limits: &wonderland_legacy_formats::Limits,
) -> Result<sim_core::vm::VmRoutine, String> {
    use sim_core::vm::{VmInstruction, VmRoutine};
    if chunk.key.kind != *b"BHAV" {
        return Err("expected a BHAV resource".into());
    }
    let bhav = wonderland_legacy_formats::semantic::decode_bhav(&chunk.data, limits)
        .map_err(|error| error.to_string())?;
    let instructions = bhav
        .instructions
        .into_iter()
        .map(|instruction| VmInstruction {
            opcode: instruction.opcode,
            true_pointer: instruction.true_pointer,
            false_pointer: instruction.false_pointer,
            operand: instruction.operand,
        })
        .collect();
    let mut routine = VmRoutine::new(
        chunk.key.id,
        bhav.locals,
        u16::from(bhav.args),
        instructions,
    )
    .map_err(|error| error.to_string())?;
    routine.routine_type = bhav.kind;
    routine.format_version = bhav.format_version;
    routine.version = bhav.tree_version;
    Ok(routine)
}

pub mod content;
pub mod isolated;

#[cfg(feature = "creator-debug")]
pub mod creator_debug;
