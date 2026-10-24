//! Error types for the Paradox63 CPU emulator.

use crate::types::{Addr, Opcode};
use thiserror::Error;

/// Errors that can occur during CPU emulation.
#[derive(Debug, Error)]
pub enum CpuError {
    /// An illegal opcode was encountered.
    #[error("illegal opcode: {0}")]
    IllegalOpcode(Opcode),

    /// A bus fault occurred during memory access.
    #[error("bus fault at address {addr}")]
    BusFault {
        /// The memory address where the fault occurred.
        addr: Addr,
        /// The underlying I/O error that caused the bus fault.
        #[source]
        source: std::io::Error,
    },

    /// A stack overflow occurred.
    #[error("stack overflow")]
    StackOverflow,

    /// Division by zero was attempted.
    #[error("divide by zero")]
    DivideByZero,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn illegal_opcode_error_message() {
        let err = CpuError::IllegalOpcode(Opcode::new(0xC5));
        assert_eq!(err.to_string(), "illegal opcode: 0xC5");
    }

    #[test]
    fn bus_fault_error_message() {
        let err = CpuError::BusFault {
            addr: Addr::new(0x00FE_DCBA),
            source: io::Error::new(io::ErrorKind::PermissionDenied, "access denied"),
        };
        assert_eq!(err.to_string(), "bus fault at address 0xFEDCBA");
    }

    #[test]
    fn bus_fault_has_source() {
        use std::error::Error;
        let source_err = io::Error::new(io::ErrorKind::NotFound, "memory not mapped");
        let err = CpuError::BusFault {
            addr: Addr::new(0x0012_3456),
            source: source_err,
        };

        assert!(err.source().is_some());
        assert_eq!(err.source().unwrap().to_string(), "memory not mapped");
    }

    #[test]
    fn stack_overflow_error_message() {
        let err = CpuError::StackOverflow;
        assert_eq!(err.to_string(), "stack overflow");
    }

    #[test]
    fn divide_by_zero_error_message() {
        let err = CpuError::DivideByZero;
        assert_eq!(err.to_string(), "divide by zero");
    }

    #[test]
    fn cpu_error_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<CpuError>();
        assert_sync::<CpuError>();
    }
}
