use crate::{alloc::vec::Vec, CallContext, ExitError, ExitReason, ExitSucceed};
use bytes::Bytes;
use primitive_types::{H160, U256};

use crate::instructions::opcode::{self, OpCode};

pub struct Contract {
    /// Contracts data
    pub input: Bytes,
    /// Contract code
    pub code: Bytes,
    /// Contract address
    pub address: H160,
    /// Caller of the EVM.
    pub caller: H160,
    /// Value send to contract.
    pub value: U256,
    /// Precomputed valid jump addresses
    jumpdest: ValidJumpAddress,
}

impl Contract {
    pub fn new(input: Bytes, code: Bytes, address: H160, caller: H160, value: U256) -> Self {
        let (jumpdest, padding) = Self::analize(code.as_ref());

        let code = if padding != 0 {
            let mut code = code.to_vec();
            code.resize(code.len() + padding, 0);
            code.into()
        } else {
            code
        };
        Self {
            input,
            code,
            address,
            caller,
            value,
            jumpdest,
        }
    }

    /// Create a new valid mapping from given code bytes.
    /// it gives back ValidJumpAddress and size od needed paddings.
    fn analize(code: &[u8]) -> (ValidJumpAddress, usize) {
        let mut jumps: Vec<bool> = Vec::with_capacity(code.len());
        jumps.resize(code.len(), false);
        let mut is_push_last = false;
        let mut i = 0;
        while i < code.len() {
            let opcode = code[i] as u8;
            if opcode == opcode::JUMPDEST as u8 {
                is_push_last = false;
                jumps[i] = true;
                i += 1;
            } else if let Some(v) = OpCode::is_push(opcode) {
                is_push_last = true;
                i += v as usize + 1;
            } else {
                is_push_last = false;
                i += 1;
            }
        }
        let padding = if is_push_last { i - code.len() } else { 0 };

        (ValidJumpAddress(jumps), padding)
    }

    pub fn is_valid_jump(&self, possition: usize) -> bool {
        self.jumpdest.is_valid(possition)
    }

    pub fn new_with_context(input: Bytes, code: Bytes, call_context: &CallContext) -> Self {
        Self::new(
            input,
            code,
            call_context.address,
            call_context.caller,
            call_context.apparent_value,
        )
    }

    pub fn opcode(&self, program_counter: usize) -> Result<OpCode, ExitReason> {
        let opcode = {
            if let Some(opcode_byte) = self.code.get(program_counter) {
                let opcode = OpCode::try_from_u8(*opcode_byte);
                // if there is no opcode in code or OpCode is invalid, return error.
                if opcode.is_none() {
                    return Err(ExitError::OpcodeNotFound.into());
                }
                opcode.unwrap()
            } else {
                return Err(ExitSucceed::Stopped.into());
            }
        };
        Ok(opcode)
    }
}

/// Mapping of valid jump destination from code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidJumpAddress(Vec<bool>);

impl ValidJumpAddress {
    /// Get the length of the valid mapping. This is the same as the
    /// code bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if the valids list is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns `true` if the position is a valid jump destination. If
    /// not, returns `false`.
    pub fn is_valid(&self, position: usize) -> bool {
        if position >= self.0.len() {
            return false;
        }

        self.0[position]
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn analize_padding_dummy() {
        let (_, padding) = Contract::analize(&[opcode::CODESIZE, opcode::PUSH1, 0x00]);
        assert_eq!(padding, 0, "Padding should be zero");
    }
    #[test]
    fn analize_padding_two_missing() {
        let (_, padding) = Contract::analize(&[opcode::CODESIZE, opcode::PUSH3, 0x00]);
        assert_eq!(padding, 2, "Padding should be zero");
    }
}
