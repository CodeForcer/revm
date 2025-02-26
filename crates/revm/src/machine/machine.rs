use crate::{
    instructions::{eval, Return},
    return_ok, return_revert,
};
use bytes::Bytes;
use core::ops::Range;

use super::{contract::Contract, memory::Memory, stack::Stack};
use crate::{spec::Spec, Host};

pub const STACK_LIMIT: u64 = 1024;
pub const CALL_STACK_LIMIT: u64 = 1024;

pub struct Machine {
    /// Contract information and invoking data
    pub contract: Contract,
    /// Program counter.
    pub program_counter: *const u8,
    /// Return value.
    pub return_range: Range<usize>,
    /// Memory.
    pub memory: Memory,
    /// Stack.
    pub stack: Stack,
    /// After call returns, its return data is saved here.
    pub return_data_buffer: Bytes,
    /// left gas. Memory gas can be found in Memory field.
    pub gas: Gas,
    /// used only for inspector.
    pub call_depth: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct Gas {
    limit: u64,
    used: u64,
    memory: u64,
    refunded: i64,
    all_used_gas: u64,
}
impl Gas {
    pub fn new(limit: u64) -> Self {
        Self {
            limit,
            used: 0,
            memory: 0,
            refunded: 0,
            all_used_gas: 0,
        }
    }

    pub fn reimburse_unspend(&mut self, exit: &Return, other: Gas) {
        match *exit {
            return_ok!() => {
                self.erase_cost(other.remaining());
                self.record_refund(other.refunded());
            }
            return_revert!() => {
                self.erase_cost(other.remaining());
            }
            _ => {}
        }
    }

    pub fn limit(&self) -> u64 {
        self.limit
    }

    pub fn memory(&self) -> u64 {
        self.memory
    }

    pub fn refunded(&self) -> i64 {
        self.refunded
    }

    pub fn spend(&self) -> u64 {
        self.all_used_gas
    }

    pub fn remaining(&self) -> u64 {
        self.limit - self.all_used_gas
    }

    pub fn erase_cost(&mut self, returned: u64) {
        self.used -= returned;
        self.all_used_gas -= returned;
    }

    pub fn record_refund(&mut self, refund: i64) {
        self.refunded += refund;
    }

    /// Record an explict cost.
    #[inline(always)]
    pub fn record_cost(&mut self, cost: u64) -> bool {
        let (all_used_gas, overflow) = self.all_used_gas.overflowing_add(cost);
        if overflow || self.limit < all_used_gas {
            return false;
        }

        self.used += cost;
        self.all_used_gas = all_used_gas;
        true
    }

    /// used in memory_resize! macro
    #[inline(always)]
    pub fn record_memory(&mut self, gas_memory: u64) -> bool {
        if gas_memory > self.memory {
            let (all_used_gas, overflow) = self.used.overflowing_add(gas_memory);
            if overflow || self.limit < all_used_gas {
                return false;
            }
            self.memory = gas_memory;
            self.all_used_gas = all_used_gas;
        }
        true
    }

    /// used in gas_refund! macro
    pub fn gas_refund(&mut self, refund: i64) {
        self.refunded += refund;
    }
}

impl Machine {
    pub fn new<SPEC: Spec>(contract: Contract, gas_limit: u64, call_depth: u64) -> Self {
        Self {
            program_counter: contract.code.as_ptr(),
            return_range: Range::default(),
            memory: Memory::new(usize::MAX),
            stack: Stack::new(),
            return_data_buffer: Bytes::new(),
            contract,
            gas: Gas::new(gas_limit),
            call_depth,
        }
    }
    pub fn contract(&self) -> &Contract {
        &self.contract
    }

    pub fn gas(&mut self) -> &Gas {
        &self.gas
    }

    /// Reference of machine stack.
    pub fn stack(&self) -> &Stack {
        &self.stack
    }

    /// Return a reference of the program counter.
    pub fn program_counter(&self) -> usize {
        unsafe { self.program_counter.offset_from(self.contract.code.as_ptr()) as usize}
    }

    /// loop steps until we are finished with execution
    pub fn run<H: Host, SPEC: Spec>(&mut self, host: &mut H) -> Return {
        let mut ret = Return::Continue;
        while ret == Return::Continue {
            ret = self.step::<H, SPEC>(host);
        }
        ret
    }

    #[inline(always)]
    /// Step the machine, executing one opcode. It then returns.
    pub fn step<H: Host, SPEC: Spec>(&mut self, host: &mut H) -> Return {
        if H::INSPECT {
            let ret = host.step(self, SPEC::IS_STATIC_CALL);
            if ret != Return::Continue {
                return ret;
            }
        }
        let opcode = unsafe {*self.program_counter};
        self.program_counter = unsafe { self.program_counter.offset(1)};
        let eval = eval::<H, SPEC>(self, opcode, host);
        
        if H::INSPECT {
            let ret = host.step_end(eval, self);
            if ret != Return::Continue {
                return ret;
            }
        }

        eval
    }

    /// Copy and get the return value of the machine, if any.
    pub fn return_value(&self) -> Bytes {
        // if start is usize max it means that our return len is zero and we need to return empty
        if self.return_range.start == usize::MAX {
            Bytes::new()
        } else {
            Bytes::copy_from_slice(self.memory.get_slice(
                self.return_range.start,
                self.return_range.end - self.return_range.start,
            ))
        }
    }
}
