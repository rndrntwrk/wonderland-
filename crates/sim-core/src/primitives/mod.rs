//! Source registry entries have separate implementation statuses from missing legacy slots.
pub mod arithmetic;
pub mod behavior;
pub mod entities;
pub mod external;
pub mod fire;
pub mod flow;
pub mod legacy;
pub mod presentation;
pub mod registry;
pub mod relationships;

use crate::vm::*;
pub use registry::*;

pub fn execute<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    instruction: &VmInstruction,
) -> Result<PrimitiveOutcome, VmFault> {
    let opcode = instruction.opcode;
    let operand = instruction.operand;
    let Some(info) = primitive_info(thread.mode, opcode) else {
        if opcode >= 256 {
            return Err(VmFault::InvalidOperand {
                opcode,
                detail: "Subroutine passed to primitive dispatcher".into(),
            });
        }
        thread.diagnostic(VmDiagnostic::MissingPrimitive { opcode });
        return Ok(PrimitiveOutcome::Exit(PrimitiveExit::GotoTrue));
    };
    let exit = match opcode {
        0 => return flow::sleep(thread, host, operand),
        2 => arithmetic::expression(thread, host, operand)?,
        3 if thread.mode == VmMode::Ts1 => behavior::find_best_action(thread, host)?,
        4 => entities::grab(thread, host)?,
        5 => entities::drop_object(thread, host)?,
        6 => presentation::change_suit(thread, host, operand)?,
        7 => presentation::refresh(thread, host, operand)?,
        8 => arithmetic::random_number(thread, host, operand)?,
        9 => fire::burn(thread, host, operand)?,
        11 => entities::distance(thread, host, operand)?,
        12 => entities::direction(thread, host, operand)?,
        13 => behavior::push_interaction(thread, host, operand)?,
        14 => behavior::find_best(thread, host, operand)?,
        15 => PrimitiveExit::GotoTrue,
        17 => return flow::idle_for_input(thread, host, operand),
        18 => entities::remove(thread, host, operand)?,
        20 => return behavior::run_functional(thread, host, operand),
        21 => presentation::show_string(thread, host, operand)?,
        24 | 26 => relationships::relationship(thread, host, opcode, operand)?,
        25 if thread.mode == VmMode::Ts1 => legacy::family_budget(thread, host, operand)?,
        28 => return behavior::run_named(thread, host, operand),
        29 => return external::motive_change(thread, host, operand),
        30 if thread.mode == VmMode::Tso => PrimitiveExit::GotoTrue,
        30 => return behavior::gosub_found_action(thread, host),
        31 => entities::set_to_next(thread, host, operand)?,
        32 => entities::test_object_type(thread, host, operand)?,
        37 => behavior::test_interacting(thread, host)?,
        41 => presentation::balloon(thread, host, operand)?,
        42 => entities::create(thread, host, operand)?,
        43 => entities::drop_onto(thread, host, operand)?,
        49 => entities::notify(thread, host)?,
        50 => presentation::change_action_string(thread, host, operand)?,
        51 if thread.mode == VmMode::Ts1 => legacy::inventory(thread, host, operand)?,
        63 => entities::terrain_info(thread, host, operand)?,
        65 => behavior::find_best_action(thread, host)?,
        44 => return external::animate(thread, operand),
        16 | 22 | 27 | 45 | 46 | 47 => return external::route(thread, host, opcode, operand),
        1 | 23 | 25 | 35 | 36 | 38 | 39 | 40 | 48 | 62 | 67 => {
            return external::external(thread, host, opcode, operand)
        }
        _ => {
            return Err(VmFault::UnsupportedPrimitive {
                opcode,
                case: format!("{} requires unported behavior", info.handler),
            })
        }
    };
    Ok(PrimitiveOutcome::Exit(exit))
}

pub(crate) fn word(bytes: &[u8; 8], offset: usize) -> i16 {
    i16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}
pub(crate) fn uword(bytes: &[u8; 8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}
pub(crate) fn dword(bytes: &[u8; 8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}
