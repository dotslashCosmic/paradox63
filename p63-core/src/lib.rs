#![warn(clippy::pedantic)]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

//! p63-core: Core ISA emulation for the Paradox63 CPU

pub mod bus;
pub mod error;
pub mod flags;
pub mod reference_interpreter;
pub mod registers;
pub mod types;

pub use bus::{Bus, Ram};
pub use error::CpuError;
pub use flags::Flags;
pub use reference_interpreter::{BlockContinuation, BlockOp, Halt, Interrupt, ReferenceInterpreter};
pub use registers::Registers;
pub use types::{Addr, Cycles, Opcode};
