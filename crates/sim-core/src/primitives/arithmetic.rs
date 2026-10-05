use super::{uword, word};
use crate::numeric;
use crate::vm::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpressionOperand {
    pub lhs: Variable,
    pub rhs: Variable,
    pub is_signed: u8,
    pub operator: u8,
}
impl ExpressionOperand {
    pub fn decode(bytes: [u8; 8]) -> Self {
        Self {
            lhs: Variable {
                scope: bytes[6] as u16,
                data: word(&bytes, 0),
            },
            rhs: Variable {
                scope: bytes[7] as u16,
                data: word(&bytes, 2),
            },
            is_signed: bytes[4],
            operator: bytes[5],
        }
    }
    pub fn encode(self) -> [u8; 8] {
        let lhs = self.lhs.data.to_le_bytes();
        let rhs = self.rhs.data.to_le_bytes();
        [
            lhs[0],
            lhs[1],
            rhs[0],
            rhs[1],
            self.is_signed,
            self.operator,
            self.lhs.scope as u8,
            self.rhs.scope as u8,
        ]
    }
}

pub fn expression<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveExit, VmFault> {
    let operand = ExpressionOperand::decode(bytes);
    let op = operand.operator;
    // IsSigned is serialized by the source decoder but never consulted by its interpreter.
    if op == 5 || op == 20 {
        let mut rhs = read_big_variable(thread, host, operand.rhs)?;
        if op == 20 {
            rhs = numeric::sqrt_i16(rhs) as i32;
        }
        return Ok(PrimitiveExit::branch(write_big_variable(
            thread,
            host,
            operand.lhs,
            rhs,
        )?));
    }
    if op == 11 || op == 17 {
        let lhs = read_big_variable(thread, host, operand.lhs)?;
        let mutated = if op == 11 {
            lhs.wrapping_add(1)
        } else {
            lhs.wrapping_sub(1)
        };
        write_big_variable(thread, host, operand.lhs, mutated)?;
        // RHS is read AFTER the write, but the comparison uses the untruncated i32 temporary.
        let rhs = read_big_variable(thread, host, operand.rhs)?;
        return Ok(PrimitiveExit::branch(if op == 11 {
            mutated < rhs
        } else {
            mutated > rhs
        }));
    }
    if (op == 18 || op == 19) && thread.mode == VmMode::Tso {
        if op == 18 {
            let entity = list_entity(thread, host, operand.lhs.scope)?;
            let mut list = host.read_list(entity)?;
            let rhs = read_big_variable(thread, host, operand.rhs)? as i16;
            match operand.lhs.data {
                0 => list.insert(0, rhs),
                1 => list.push(rhs),
                2 => {
                    return Err(VmFault::InvalidOperand {
                        opcode: 2,
                        detail: "Unknown list push destination 2".into(),
                    })
                }
                _ => return Ok(PrimitiveExit::GotoTrue),
            }
            host.replace_list(entity, list)?;
        } else {
            let entity = list_entity(thread, host, operand.rhs.scope)?;
            let mut list = host.read_list(entity)?;
            if list.is_empty() {
                return Ok(PrimitiveExit::GotoFalse);
            }
            let value = match operand.rhs.data {
                0 => list.remove(0),
                1 => list.pop().expect("nonempty list"),
                2 => {
                    return Err(VmFault::InvalidOperand {
                        opcode: 2,
                        detail: "Unknown list pop source 2".into(),
                    })
                }
                _ => 0,
            };
            host.replace_list(entity, list)?;
            // Source consumes the item even if the destination setter returns false.
            write_big_variable(thread, host, operand.lhs, value as i32)?;
        }
        return Ok(PrimitiveExit::GotoTrue);
    }
    let lhs = read_big_variable(thread, host, operand.lhs)?;
    let rhs = read_big_variable(thread, host, operand.rhs)?;
    if [0, 1, 2, 8, 14, 15, 16].contains(&op) {
        let result = if (op == 1 || op == 14)
            && rhs == 1024
            && operand.lhs.scope == 3
            && operand.lhs.data == 29
        {
            let frame = thread.top()?;
            let caller = host.entity_info(frame.context.caller)?;
            let callee = host.entity_info(frame.context.callee)?;
            let relative = caller.position.level as i32
                - (callee.position.level as i32 - callee.level_offset as i32);
            if op == 1 {
                relative <= 0
            } else {
                relative > 0
            }
        } else {
            match op {
                0 => lhs > rhs,
                1 => lhs < rhs,
                2 => lhs == rhs,
                8 => numeric::is_flag_set_i32(lhs, rhs),
                14 => lhs >= rhs,
                15 => lhs <= rhs,
                16 => lhs != rhs,
                _ => unreachable!(),
            }
        };
        return Ok(PrimitiveExit::branch(result));
    }
    let value =
        match op {
            3 => lhs.wrapping_add(rhs),
            4 => lhs.wrapping_sub(rhs),
            6 => lhs.wrapping_mul(rhs),
            7 => numeric::div_i32(lhs, rhs)
                .map_err(|error| VmFault::Arithmetic(format!("{error:?}")))?,
            9 => numeric::set_flag_i32(lhs, rhs),
            10 => numeric::clear_flag_i32(lhs, rhs),
            12 => numeric::mod_i32(lhs, rhs)
                .map_err(|error| VmFault::Arithmetic(format!("{error:?}")))?,
            13 => lhs & rhs,
            18 => lhs | rhs,
            19 => lhs ^ rhs,
            _ => {
                return Err(VmFault::InvalidOperand {
                    opcode: 2,
                    detail: format!("Unknown expression operator {op}"),
                })
            }
        };
    let accepted = write_big_variable(thread, host, operand.lhs, value)?;
    // Only SetFlag/ClearFlag/Assign/sqrt branch on the setter's success.
    Ok(if op == 9 || op == 10 {
        PrimitiveExit::branch(accepted)
    } else {
        PrimitiveExit::GotoTrue
    })
}

pub fn random_number<H: VmHost + ?Sized>(
    thread: &mut VmThread,
    host: &mut H,
    bytes: [u8; 8],
) -> Result<PrimitiveExit, VmFault> {
    let destination = Variable {
        data: word(&bytes, 0),
        scope: uword(&bytes, 2),
    };
    let range = Variable {
        data: word(&bytes, 4),
        scope: uword(&bytes, 6),
    };
    let bound = read_variable(thread, host, range)? as u16;
    let result = host.next_random(bound as u64) as i16;
    write_variable(thread, host, destination, result)?;
    Ok(PrimitiveExit::GotoTrue)
}
