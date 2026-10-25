//! Data transfer instruction handlers for the reference interpreter.

use crate::{Addr, Bus, Cycles, CpuError, Flags};
use super::operand::odd_penalty;
use super::ReferenceInterpreter;

impl<B: Bus> ReferenceInterpreter<B> {

    /// Decode immediate 16-bit value (little-endian) from memory after postbyte
    fn decode_imm16(&mut self, postbyte: u8) -> u16 {
        // postbyte is LSB, next byte is MSB
        let lsb = postbyte;
        let msb = self.bus.read(Addr::new(self.registers.pc()));
        self.registers.set_pc(self.registers.pc() + 1);
        u16::from(lsb) | (u16::from(msb) << 8)
    }

    /// Decode immediate 24-bit value (little-endian) from memory
    #[allow(dead_code)]
    fn decode_imm24(&mut self) -> u32 {
        let pc = self.registers.pc();
        let byte0 = self.bus.read(Addr::new(pc));
        let byte1 = self.bus.read(Addr::new(pc + 1));
        let byte2 = self.bus.read(Addr::new(pc + 2));
        self.registers.set_pc(pc + 3);
        u32::from(byte0) | (u32::from(byte1) << 8) | (u32::from(byte2) << 16)
    }

    /// Decode immediate 32-bit value (little-endian) from memory
    #[allow(dead_code)]
    fn decode_imm32(&mut self) -> u32 {
        let pc = self.registers.pc();
        let byte0 = self.bus.read(Addr::new(pc));
        let byte1 = self.bus.read(Addr::new(pc + 1));
        let byte2 = self.bus.read(Addr::new(pc + 2));
        let byte3 = self.bus.read(Addr::new(pc + 3));
        self.registers.set_pc(pc + 4);
        u32::from(byte0) | (u32::from(byte1) << 8) | (u32::from(byte2) << 16) | (u32::from(byte3) << 24)
    }

    /// Decode extended 24-bit address from postbyte + 2 more bytes
    fn decode_extended(&mut self, postbyte: u8) -> u32 {
        let pc = self.registers.pc();
        let byte1 = self.bus.read(Addr::new(pc));
        let byte2 = self.bus.read(Addr::new(pc + 1));
        self.registers.set_pc(pc + 2);

        // Little-endian: postbyte is bits 0-7, byte1 is bits 8-15, byte2 is bits 16-23
        u32::from(postbyte) | (u32::from(byte1) << 8) | (u32::from(byte2) << 16)
    }

    /// Update N and Z flags based on an 8-bit result
    fn update_nz_8(&mut self, result: u8) {
        let mut flags = self.registers.cc();
        flags.set(Flags::N, result & 0x80 != 0);
        flags.set(Flags::Z, result == 0);
        self.registers.set_cc(flags);
    }

    /// Update N and Z flags based on a 16-bit result
    fn update_nz_16(&mut self, result: u16) {
        let mut flags = self.registers.cc();
        flags.set(Flags::N, result & 0x8000 != 0);
        flags.set(Flags::Z, result == 0);
        self.registers.set_cc(flags);
    }

    /// Update N and Z flags based on a 32-bit result
    fn update_nz_24(&mut self, result: u32) {
        let result = result & 0x00FF_FFFF;
        let mut flags = self.registers.cc();
        flags.set(Flags::N, result & 0x0080_0000 != 0);
        flags.set(Flags::Z, result == 0);
        self.registers.set_cc(flags);
    }

    fn update_nz_32(&mut self, result: u32) {
        let mut flags = self.registers.cc();
        flags.set(Flags::N, result & 0x8000_0000 != 0);
        flags.set(Flags::Z, result == 0);
        self.registers.set_cc(flags);
    }

    // Instruction handlers - Data Transfer group

    /// LD A, #imm — Load A immediate (opcode 0x01, 2 cycles)
    pub(super) fn exec_ld_a_imm(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let value = postbyte; // 8-bit immediate is the postbyte
        self.registers.set_a(value);
        self.update_nz_8(value);
        Ok(Cycles(2))
    }

    /// LD B, #imm — Load B immediate (opcode 0x02, 2 cycles)
    pub(super) fn exec_ld_b_imm(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let value = postbyte; // 8-bit immediate is the postbyte
        self.registers.set_b(value);
        self.update_nz_8(value);
        Ok(Cycles(2))
    }

    /// LD D, #imm — Load D immediate (opcode 0x03, 2 cycles)
    pub(super) fn exec_ld_d_imm(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let value = self.decode_imm16(postbyte);
        self.registers.set_d(value);
        self.update_nz_16(value);
        Ok(Cycles(2))
    }

    /// LD E, #imm — Load E immediate (opcode 0x04, 2 cycles)
    pub(super) fn exec_ld_e_imm(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let value = postbyte; // 8-bit immediate is the postbyte
        self.registers.set_e(value);
        self.update_nz_8(value);
        Ok(Cycles(2))
    }

    /// LD F, #imm — Load F immediate (opcode 0x05, 2 cycles)
    pub(super) fn exec_ld_f_imm(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let value = postbyte; // 8-bit immediate is the postbyte
        self.registers.set_f(value);
        self.update_nz_8(value);
        Ok(Cycles(2))
    }

    /// LD W, #imm — Load W immediate (opcode 0x06, 2 cycles)
    pub(super) fn exec_ld_w_imm(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let value = self.decode_imm16(postbyte);
        self.registers.set_w(value);
        self.update_nz_16(value);
        Ok(Cycles(2))
    }

    /// LD Q, #imm — Load Q immediate (opcode 0x07, 3 cycles)
    pub(super) fn exec_ld_q_imm(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        // Q is 32-bit, so postbyte is the first byte
        let pc = self.registers.pc();
        let byte1 = self.bus.read(Addr::new(pc));
        let byte2 = self.bus.read(Addr::new(pc + 1));
        let byte3 = self.bus.read(Addr::new(pc + 2));
        self.registers.set_pc(pc + 3);

        let value = u32::from(postbyte) | (u32::from(byte1) << 8) | (u32::from(byte2) << 16) | (u32::from(byte3) << 24);
        self.registers.set_q(value);
        self.update_nz_32(value);
        Ok(Cycles(3))
    }

    /// MOV reg, reg — Move register to register (opcode 0x08, 1 cycle)
    /// Postbyte format: [`src_reg` 4 bits][dest_reg 4 bits]
    pub(super) fn exec_mov(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let src_reg = (postbyte >> 4) & 0x0F;
        let dest_reg = postbyte & 0x0F;

        // Read source register value (extended to 32-bit to handle all sizes)
        let value = match src_reg {
            0x0 => u32::from(self.registers.a()),      // A
            0x1 => u32::from(self.registers.b()),      // B
            0x2 => u32::from(self.registers.d()),      // D
            0x3 => u32::from(self.registers.e()),      // E
            0x4 => u32::from(self.registers.f()),      // F
            0x5 => u32::from(self.registers.w()),      // W
            0x6 => self.registers.q(),                 // Q
            0x7 => u32::from(self.registers.gp()),     // GP
            0x8 => self.registers.x(),                 // X
            0x9 => self.registers.y(),                 // Y
            0xA => self.registers.sp(),                // SP
            0xB => self.registers.pc(),                // PC
            0xC => u32::from(self.registers.dp()),     // DP
            0xD => u32::from(self.registers.cc().bits()), // CC
            _ => return Err(CpuError::IllegalOpcode(crate::Opcode::new(0x08))), // Reserved
        };

        // Write to destination register (with appropriate truncation/extension)
        match dest_reg {
            0x0 => {
                let val = value as u8;
                self.registers.set_a(val);
                self.update_nz_8(val);
            },
            0x1 => {
                let val = value as u8;
                self.registers.set_b(val);
                self.update_nz_8(val);
            },
            0x2 => {
                let val = value as u16;
                self.registers.set_d(val);
                self.update_nz_16(val);
            },
            0x3 => {
                let val = value as u8;
                self.registers.set_e(val);
                self.update_nz_8(val);
            },
            0x4 => {
                let val = value as u8;
                self.registers.set_f(val);
                self.update_nz_8(val);
            },
            0x5 => {
                let val = value as u16;
                self.registers.set_w(val);
                self.update_nz_16(val);
            },
            0x6 => {
                self.registers.set_q(value);
                self.update_nz_32(value);
            },
            0x7 => {
                let val = value as u8;
                self.registers.set_gp(val);
                self.update_nz_8(val);
            },
            0x8 => {
                self.registers.set_x(value);
                self.update_nz_24(value);
            },
            0x9 => {
                self.registers.set_y(value);
                self.update_nz_24(value);
            },
            0xA => {
                self.registers.set_sp(value);
                self.update_nz_24(value);
            },
            0xB => {
                // a jump, so the flags stay
                self.registers.set_pc(value);
            },
            0xC => {
                let val = value as u8;
                self.registers.set_dp(val);
                self.update_nz_8(val);
            },
            0xD => {
                let val = Flags::from_bits_truncate(value as u8);
                self.registers.set_cc(val);
                // No flags updated (would be circular)
            },
            _ => return Err(CpuError::IllegalOpcode(crate::Opcode::new(0x08))), // Reserved
        }

        Ok(Cycles(1))
    }

    /// ST A, addr — Store A to direct page (opcode 0x09, 3 cycles)
    pub(super) fn exec_st_a(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if (postbyte & 0xC0) != 0 {
            // Illegal postbyte for direct page - execute as 1-cycle NOP
            return Ok(Cycles(1));
        }

        let offset = postbyte & 0x3F;
        let addr = (u32::from(self.registers.dp()) << 8) | u32::from(offset);
        let addr = addr & 0x00FF_FFFF; // Mask to 24 bits
        let value = self.registers.a();
        self.bus.write(Addr::new(addr), value);
        Ok(Cycles(3))
    }

    /// ST B, addr — Store B to direct page (opcode 0x0A, 3 cycles)
    pub(super) fn exec_st_b(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if (postbyte & 0xC0) != 0 {
            // Illegal postbyte for direct page - execute as 1-cycle NOP
            return Ok(Cycles(1));
        }

        let offset = postbyte & 0x3F;
        let addr = (u32::from(self.registers.dp()) << 8) | u32::from(offset);
        let addr = addr & 0x00FF_FFFF; // Mask to 24 bits
        let value = self.registers.b();
        self.bus.write(Addr::new(addr), value);
        Ok(Cycles(3))
    }

    /// ST D, addr — Store D to direct page (opcode 0x0B, 3 cycles)
    pub(super) fn exec_st_d(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if (postbyte & 0xC0) != 0 {
            // Illegal postbyte for direct page - execute as 1-cycle NOP
            return Ok(Cycles(1));
        }

        let offset = postbyte & 0x3F;
        let addr = (u32::from(self.registers.dp()) << 8) | u32::from(offset);
        let addr = addr & 0x00FF_FFFF; // Mask to 24 bits
        let value = self.registers.d();
        // Little-endian: LSB at lower address
        self.bus.write(Addr::new(addr), value as u8);
        self.bus.write(Addr::new(addr + 1), (value >> 8) as u8);
        Ok(Cycles(3 + odd_penalty(addr, 2)))
    }

    /// ST E, addr — Store E to direct page (opcode 0x0C, 3 cycles)
    pub(super) fn exec_st_e(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if (postbyte & 0xC0) != 0 {
            // Illegal postbyte for direct page - execute as 1-cycle NOP
            return Ok(Cycles(1));
        }

        let offset = postbyte & 0x3F;
        let addr = (u32::from(self.registers.dp()) << 8) | u32::from(offset);
        let addr = addr & 0x00FF_FFFF; // Mask to 24 bits
        let value = self.registers.e();
        self.bus.write(Addr::new(addr), value);
        Ok(Cycles(3))
    }

    /// ST F, addr — Store F to direct page (opcode 0x0D, 3 cycles)
    pub(super) fn exec_st_f(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if (postbyte & 0xC0) != 0 {
            // Illegal postbyte for direct page - execute as 1-cycle NOP
            return Ok(Cycles(1));
        }

        let offset = postbyte & 0x3F;
        let addr = (u32::from(self.registers.dp()) << 8) | u32::from(offset);
        let addr = addr & 0x00FF_FFFF; // Mask to 24 bits
        let value = self.registers.f();
        self.bus.write(Addr::new(addr), value);
        Ok(Cycles(3))
    }

    /// ST W, addr — Store W to direct page (opcode 0x0E, 3 cycles)
    pub(super) fn exec_st_w(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if (postbyte & 0xC0) != 0 {
            // Illegal postbyte for direct page - execute as 1-cycle NOP
            return Ok(Cycles(1));
        }

        let offset = postbyte & 0x3F;
        let addr = (u32::from(self.registers.dp()) << 8) | u32::from(offset);
        let addr = addr & 0x00FF_FFFF; // Mask to 24 bits
        let value = self.registers.w();
        // Little-endian: LSB at lower address
        self.bus.write(Addr::new(addr), value as u8);
        self.bus.write(Addr::new(addr + 1), (value >> 8) as u8);
        Ok(Cycles(3 + odd_penalty(addr, 2)))
    }

    /// ST Q, addr — Store Q to direct page (opcode 0x0F, 4 cycles)
    pub(super) fn exec_st_q(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if (postbyte & 0xC0) != 0 {
            // Illegal postbyte for direct page - execute as 1-cycle NOP
            return Ok(Cycles(1));
        }

        let offset = postbyte & 0x3F;
        let addr = (u32::from(self.registers.dp()) << 8) | u32::from(offset);
        let addr = addr & 0x00FF_FFFF; // Mask to 24 bits
        let value = self.registers.q();
        // Little-endian: LSB at lower address
        self.bus.write(Addr::new(addr), value as u8);
        self.bus.write(Addr::new(addr + 1), (value >> 8) as u8);
        self.bus.write(Addr::new(addr + 2), (value >> 16) as u8);
        self.bus.write(Addr::new(addr + 3), (value >> 24) as u8);
        Ok(Cycles(4 + odd_penalty(addr, 4)))
    }

    /// LD A, addr — Load A from direct page (opcode 0x46, 3 cycles)
    pub(super) fn exec_ld_a_mem(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if (postbyte & 0xC0) != 0 {
            // Illegal postbyte for direct page - execute as 1-cycle NOP
            return Ok(Cycles(1));
        }

        let offset = postbyte & 0x3F;
        let addr = (u32::from(self.registers.dp()) << 8) | u32::from(offset);
        let addr = addr & 0x00FF_FFFF; // Mask to 24 bits
        let value = self.bus.read(Addr::new(addr));
        self.registers.set_a(value);
        self.update_nz_8(value);
        Ok(Cycles(3))
    }

    /// LD B, addr — Load B from direct page (opcode 0x47, 3 cycles)
    pub(super) fn exec_ld_b_mem(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if (postbyte & 0xC0) != 0 {
            // Illegal postbyte for direct page - execute as 1-cycle NOP
            return Ok(Cycles(1));
        }

        let offset = postbyte & 0x3F;
        let addr = (u32::from(self.registers.dp()) << 8) | u32::from(offset);
        let addr = addr & 0x00FF_FFFF; // Mask to 24 bits
        let value = self.bus.read(Addr::new(addr));
        self.registers.set_b(value);
        self.update_nz_8(value);
        Ok(Cycles(3))
    }

    /// LD D, addr — Load D from direct page (opcode 0x48, 3 cycles)
    pub(super) fn exec_ld_d_mem(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if (postbyte & 0xC0) != 0 {
            // Illegal postbyte for direct page - execute as 1-cycle NOP
            return Ok(Cycles(1));
        }

        let offset = postbyte & 0x3F;
        let addr = (u32::from(self.registers.dp()) << 8) | u32::from(offset);
        let addr = addr & 0x00FF_FFFF; // Mask to 24 bits
        let lsb = self.bus.read(Addr::new(addr));
        let msb = self.bus.read(Addr::new(addr + 1));
        let value = u16::from(lsb) | (u16::from(msb) << 8);
        self.registers.set_d(value);
        self.update_nz_16(value);
        Ok(Cycles(3 + odd_penalty(addr, 2)))
    }

    /// LD E, addr — Load E from direct page (opcode 0x49, 3 cycles)
    pub(super) fn exec_ld_e_mem(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if (postbyte & 0xC0) != 0 {
            // Illegal postbyte for direct page - execute as 1-cycle NOP
            return Ok(Cycles(1));
        }

        let offset = postbyte & 0x3F;
        let addr = (u32::from(self.registers.dp()) << 8) | u32::from(offset);
        let addr = addr & 0x00FF_FFFF; // Mask to 24 bits
        let value = self.bus.read(Addr::new(addr));
        self.registers.set_e(value);
        self.update_nz_8(value);
        Ok(Cycles(3))
    }

    /// LD F, addr — Load F from direct page (opcode 0x4A, 3 cycles)
    pub(super) fn exec_ld_f_mem(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if (postbyte & 0xC0) != 0 {
            // Illegal postbyte for direct page - execute as 1-cycle NOP
            return Ok(Cycles(1));
        }

        let offset = postbyte & 0x3F;
        let addr = (u32::from(self.registers.dp()) << 8) | u32::from(offset);
        let addr = addr & 0x00FF_FFFF; // Mask to 24 bits
        let value = self.bus.read(Addr::new(addr));
        self.registers.set_f(value);
        self.update_nz_8(value);
        Ok(Cycles(3))
    }

    /// LD W, addr — Load W from direct page (opcode 0x4B, 3 cycles)
    pub(super) fn exec_ld_w_mem(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if (postbyte & 0xC0) != 0 {
            // Illegal postbyte for direct page - execute as 1-cycle NOP
            return Ok(Cycles(1));
        }

        let offset = postbyte & 0x3F;
        let addr = (u32::from(self.registers.dp()) << 8) | u32::from(offset);
        let addr = addr & 0x00FF_FFFF; // Mask to 24 bits
        let lsb = self.bus.read(Addr::new(addr));
        let msb = self.bus.read(Addr::new(addr + 1));
        let value = u16::from(lsb) | (u16::from(msb) << 8);
        self.registers.set_w(value);
        self.update_nz_16(value);
        Ok(Cycles(3 + odd_penalty(addr, 2)))
    }

    /// LD Q, addr — Load Q from direct page (opcode 0x4C, 4 cycles)
    pub(super) fn exec_ld_q_mem(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if (postbyte & 0xC0) != 0 {
            // Illegal postbyte for direct page - execute as 1-cycle NOP
            return Ok(Cycles(1));
        }

        let offset = postbyte & 0x3F;
        let addr = (u32::from(self.registers.dp()) << 8) | u32::from(offset);
        let addr = addr & 0x00FF_FFFF; // Mask to 24 bits
        let byte0 = self.bus.read(Addr::new(addr));
        let byte1 = self.bus.read(Addr::new(addr + 1));
        let byte2 = self.bus.read(Addr::new(addr + 2));
        let byte3 = self.bus.read(Addr::new(addr + 3));
        let value = u32::from(byte0) | (u32::from(byte1) << 8) | (u32::from(byte2) << 16) | (u32::from(byte3) << 24);
        self.registers.set_q(value);
        self.update_nz_32(value);
        Ok(Cycles(4 + odd_penalty(addr, 4)))
    }

    /// EXG reg, reg — Exchange registers (opcode 0xA0, 1 cycle)
    /// Postbyte format: [`src_reg` 4 bits][dest_reg 4 bits]
    pub(super) fn exec_exg(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let reg1 = (postbyte >> 4) & 0x0F;
        let reg2 = postbyte & 0x0F;

        // Read both register values
        let val1 = match reg1 {
            0x0 => u32::from(self.registers.a()),
            0x1 => u32::from(self.registers.b()),
            0x2 => u32::from(self.registers.d()),
            0x3 => u32::from(self.registers.e()),
            0x4 => u32::from(self.registers.f()),
            0x5 => u32::from(self.registers.w()),
            0x6 => self.registers.q(),
            0x7 => u32::from(self.registers.gp()),
            0x8 => self.registers.x(),
            0x9 => self.registers.y(),
            0xA => self.registers.sp(),
            0xB => self.registers.pc(),
            0xC => u32::from(self.registers.dp()),
            0xD => u32::from(self.registers.cc().bits()),
            _ => return Err(CpuError::IllegalOpcode(crate::Opcode::new(0xA0))),
        };

        let val2 = match reg2 {
            0x0 => u32::from(self.registers.a()),
            0x1 => u32::from(self.registers.b()),
            0x2 => u32::from(self.registers.d()),
            0x3 => u32::from(self.registers.e()),
            0x4 => u32::from(self.registers.f()),
            0x5 => u32::from(self.registers.w()),
            0x6 => self.registers.q(),
            0x7 => u32::from(self.registers.gp()),
            0x8 => self.registers.x(),
            0x9 => self.registers.y(),
            0xA => self.registers.sp(),
            0xB => self.registers.pc(),
            0xC => u32::from(self.registers.dp()),
            0xD => u32::from(self.registers.cc().bits()),
            _ => return Err(CpuError::IllegalOpcode(crate::Opcode::new(0xA0))),
        };

        // Write exchanged values with truncation
        match reg1 {
            0x0 => self.registers.set_a(val2 as u8),
            0x1 => self.registers.set_b(val2 as u8),
            0x2 => self.registers.set_d(val2 as u16),
            0x3 => self.registers.set_e(val2 as u8),
            0x4 => self.registers.set_f(val2 as u8),
            0x5 => self.registers.set_w(val2 as u16),
            0x6 => self.registers.set_q(val2),
            0x7 => self.registers.set_gp(val2 as u8),
            0x8 => self.registers.set_x(val2),
            0x9 => self.registers.set_y(val2),
            0xA => self.registers.set_sp(val2),
            0xB => self.registers.set_pc(val2),
            0xC => self.registers.set_dp(val2 as u8),
            0xD => self.registers.set_cc(Flags::from_bits_truncate(val2 as u8)),
            _ => unreachable!(),
        }

        match reg2 {
            0x0 => self.registers.set_a(val1 as u8),
            0x1 => self.registers.set_b(val1 as u8),
            0x2 => self.registers.set_d(val1 as u16),
            0x3 => self.registers.set_e(val1 as u8),
            0x4 => self.registers.set_f(val1 as u8),
            0x5 => self.registers.set_w(val1 as u16),
            0x6 => self.registers.set_q(val1),
            0x7 => self.registers.set_gp(val1 as u8),
            0x8 => self.registers.set_x(val1),
            0x9 => self.registers.set_y(val1),
            0xA => self.registers.set_sp(val1),
            0xB => self.registers.set_pc(val1),
            0xC => self.registers.set_dp(val1 as u8),
            0xD => self.registers.set_cc(Flags::from_bits_truncate(val1 as u8)),
            _ => unreachable!(),
        }

        Ok(Cycles(1))
    }

    /// PUSH reg — Push register to stack (opcode 0x56, 2-3 cycles)
    /// Postbyte encodes which register to push
    pub(super) fn exec_push(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let reg = postbyte & 0x0F;

        match reg {
            0x0 => {
                // Push A (8-bit, 1 byte)
                let sp = self.registers.sp().wrapping_sub(1) & 0x00FF_FFFF;
                self.registers.set_sp(sp);
                self.bus.write(Addr::new(sp), self.registers.a());
                Ok(Cycles(2))
            }
            0x1 => {
                // Push B (8-bit, 1 byte)
                let sp = self.registers.sp().wrapping_sub(1) & 0x00FF_FFFF;
                self.registers.set_sp(sp);
                self.bus.write(Addr::new(sp), self.registers.b());
                Ok(Cycles(2))
            }
            0x2 => {
                // Push D (16-bit, 2 bytes)
                let value = self.registers.d();
                let sp = self.registers.sp().wrapping_sub(2) & 0x00FF_FFFF;
                self.registers.set_sp(sp);
                self.bus.write(Addr::new(sp), value as u8);
                self.bus.write(Addr::new(sp + 1), (value >> 8) as u8);
                Ok(Cycles(2 + odd_penalty(sp, 2)))
            }
            0x3 => {
                // Push E (8-bit, 1 byte)
                let sp = self.registers.sp().wrapping_sub(1) & 0x00FF_FFFF;
                self.registers.set_sp(sp);
                self.bus.write(Addr::new(sp), self.registers.e());
                Ok(Cycles(2))
            }
            0x4 => {
                // Push F (8-bit, 1 byte)
                let sp = self.registers.sp().wrapping_sub(1) & 0x00FF_FFFF;
                self.registers.set_sp(sp);
                self.bus.write(Addr::new(sp), self.registers.f());
                Ok(Cycles(2))
            }
            0x5 => {
                // Push W (16-bit, 2 bytes)
                let value = self.registers.w();
                let sp = self.registers.sp().wrapping_sub(2) & 0x00FF_FFFF;
                self.registers.set_sp(sp);
                self.bus.write(Addr::new(sp), value as u8);
                self.bus.write(Addr::new(sp + 1), (value >> 8) as u8);
                Ok(Cycles(2 + odd_penalty(sp, 2)))
            }
            0x6 => {
                // Push Q (32-bit, 4 bytes)
                let value = self.registers.q();
                let sp = self.registers.sp().wrapping_sub(4) & 0x00FF_FFFF;
                self.registers.set_sp(sp);
                self.bus.write(Addr::new(sp), value as u8);
                self.bus.write(Addr::new(sp + 1), (value >> 8) as u8);
                self.bus.write(Addr::new(sp + 2), (value >> 16) as u8);
                self.bus.write(Addr::new(sp + 3), (value >> 24) as u8);
                Ok(Cycles(3 + odd_penalty(sp, 4)))
            }
            0x7 => {
                // Push GP (8-bit, 1 byte)
                let sp = self.registers.sp().wrapping_sub(1) & 0x00FF_FFFF;
                self.registers.set_sp(sp);
                self.bus.write(Addr::new(sp), self.registers.gp());
                Ok(Cycles(2))
            }
            0x8 => {
                // Push X (24-bit, 3 bytes)
                let value = self.registers.x();
                let sp = self.registers.sp().wrapping_sub(3) & 0x00FF_FFFF;
                self.registers.set_sp(sp);
                self.bus.write(Addr::new(sp), value as u8);
                self.bus.write(Addr::new(sp + 1), (value >> 8) as u8);
                self.bus.write(Addr::new(sp + 2), (value >> 16) as u8);
                Ok(Cycles(3))
            }
            0x9 => {
                // Push Y (24-bit, 3 bytes)
                let value = self.registers.y();
                let sp = self.registers.sp().wrapping_sub(3) & 0x00FF_FFFF;
                self.registers.set_sp(sp);
                self.bus.write(Addr::new(sp), value as u8);
                self.bus.write(Addr::new(sp + 1), (value >> 8) as u8);
                self.bus.write(Addr::new(sp + 2), (value >> 16) as u8);
                Ok(Cycles(3))
            }
            0xA => {
                // Push SP (24-bit, 3 bytes) - pushes pre-decrement value
                let value = self.registers.sp();
                let sp = value.wrapping_sub(3) & 0x00FF_FFFF;
                self.registers.set_sp(sp);
                self.bus.write(Addr::new(sp), value as u8);
                self.bus.write(Addr::new(sp + 1), (value >> 8) as u8);
                self.bus.write(Addr::new(sp + 2), (value >> 16) as u8);
                Ok(Cycles(3))
            }
            0xC | 0xD => {
                // Push DP or CC (8-bit, 1 byte), as PUSH2 already allows
                let value = self.read_register(reg)? as u8;
                let sp = self.registers.sp().wrapping_sub(1) & 0x00FF_FFFF;
                self.registers.set_sp(sp);
                self.bus.write(Addr::new(sp), value);
                Ok(Cycles(2))
            }
            _ => Err(CpuError::IllegalOpcode(crate::Opcode::new(0x56))),
        }
    }

    /// POP reg — Pop from stack to register (opcode 0x58, 2-3 cycles)
    /// Postbyte encodes which register to pop to
    #[allow(clippy::too_many_lines)] // one arm per register
    pub(super) fn exec_pop(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let reg = postbyte & 0x0F;

        match reg {
            0x0 => {
                // Pop A (8-bit, 1 byte)
                let sp = self.registers.sp();
                let value = self.bus.read(Addr::new(sp));
                self.registers.set_sp(sp + 1);
                self.registers.set_a(value);
                Ok(Cycles(2))
            }
            0x1 => {
                // Pop B (8-bit, 1 byte)
                let sp = self.registers.sp();
                let value = self.bus.read(Addr::new(sp));
                self.registers.set_sp(sp + 1);
                self.registers.set_b(value);
                Ok(Cycles(2))
            }
            0x2 => {
                // Pop D (16-bit, 2 bytes)
                let sp = self.registers.sp();
                let lsb = self.bus.read(Addr::new(sp));
                let msb = self.bus.read(Addr::new(sp + 1));
                let value = u16::from(lsb) | (u16::from(msb) << 8);
                self.registers.set_sp(sp + 2);
                self.registers.set_d(value);
                Ok(Cycles(2 + odd_penalty(sp, 2)))
            }
            0x3 => {
                // Pop E (8-bit, 1 byte)
                let sp = self.registers.sp();
                let value = self.bus.read(Addr::new(sp));
                self.registers.set_sp(sp + 1);
                self.registers.set_e(value);
                Ok(Cycles(2))
            }
            0x4 => {
                // Pop F (8-bit, 1 byte)
                let sp = self.registers.sp();
                let value = self.bus.read(Addr::new(sp));
                self.registers.set_sp(sp + 1);
                self.registers.set_f(value);
                Ok(Cycles(2))
            }
            0x5 => {
                // Pop W (16-bit, 2 bytes)
                let sp = self.registers.sp();
                let lsb = self.bus.read(Addr::new(sp));
                let msb = self.bus.read(Addr::new(sp + 1));
                let value = u16::from(lsb) | (u16::from(msb) << 8);
                self.registers.set_sp(sp + 2);
                self.registers.set_w(value);
                Ok(Cycles(2 + odd_penalty(sp, 2)))
            }
            0x6 => {
                // Pop Q (32-bit, 4 bytes)
                let sp = self.registers.sp();
                let byte0 = self.bus.read(Addr::new(sp));
                let byte1 = self.bus.read(Addr::new(sp + 1));
                let byte2 = self.bus.read(Addr::new(sp + 2));
                let byte3 = self.bus.read(Addr::new(sp + 3));
                let value = u32::from(byte0) | (u32::from(byte1) << 8)
                    | (u32::from(byte2) << 16) | (u32::from(byte3) << 24);
                self.registers.set_sp(sp + 4);
                self.registers.set_q(value);
                Ok(Cycles(3 + odd_penalty(sp, 4)))
            }
            0x7 => {
                // Pop GP (8-bit, 1 byte)
                let sp = self.registers.sp();
                let value = self.bus.read(Addr::new(sp));
                self.registers.set_sp(sp + 1);
                self.registers.set_gp(value);
                Ok(Cycles(2))
            }
            0x8 => {
                // Pop X (24-bit, 3 bytes)
                let sp = self.registers.sp();
                let byte0 = self.bus.read(Addr::new(sp));
                let byte1 = self.bus.read(Addr::new(sp + 1));
                let byte2 = self.bus.read(Addr::new(sp + 2));
                let value = u32::from(byte0) | (u32::from(byte1) << 8) | (u32::from(byte2) << 16);
                self.registers.set_sp(sp + 3);
                self.registers.set_x(value);
                Ok(Cycles(3))
            }
            0x9 => {
                // Pop Y (24-bit, 3 bytes)
                let sp = self.registers.sp();
                let byte0 = self.bus.read(Addr::new(sp));
                let byte1 = self.bus.read(Addr::new(sp + 1));
                let byte2 = self.bus.read(Addr::new(sp + 2));
                let value = u32::from(byte0) | (u32::from(byte1) << 8) | (u32::from(byte2) << 16);
                self.registers.set_sp(sp + 3);
                self.registers.set_y(value);
                Ok(Cycles(3))
            }
            0xA => {
                // Pop SP (24-bit, 3 bytes)
                let sp = self.registers.sp();
                let byte0 = self.bus.read(Addr::new(sp));
                let byte1 = self.bus.read(Addr::new(sp + 1));
                let byte2 = self.bus.read(Addr::new(sp + 2));
                let value = u32::from(byte0) | (u32::from(byte1) << 8) | (u32::from(byte2) << 16);
                self.registers.set_sp(value);
                Ok(Cycles(3))
            }
            0xC | 0xD => {
                // Pop DP or CC (8-bit, 1 byte), as POP2 already allows
                let sp = self.registers.sp();
                let value = self.bus.read(Addr::new(sp));
                self.registers.set_sp(sp + 1);
                self.write_register(reg, u32::from(value))?;
                Ok(Cycles(2))
            }
            _ => Err(CpuError::IllegalOpcode(crate::Opcode::new(0x58))),
        }
    }

    /// PUSH2 reg1, reg2 — Push two registers to stack (opcode 0x57, 3-4 cycles)
    /// Postbyte format: [reg1 4 bits][reg2 4 bits]
    /// Push order: reg1 first (lower in memory), then reg2 (on top)
    /// Cycle count: 3 if both are 8-bit or 16-bit, 4 if either is 24-bit or 32-bit
    pub(super) fn exec_push2(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let reg1 = (postbyte >> 4) & 0x0F;
        let reg2 = postbyte & 0x0F;

        // Read both register values
        let value1 = self.read_register(reg1)?;
        let value2 = self.read_register(reg2)?;

        // Calculate sizes
        let size1 = Self::register_size(reg1);
        let size2 = Self::register_size(reg2);
        let total_size = size1 + size2;

        // Determine cycle count: 4 if either register is 24-bit or 32-bit, else 3
        let cycles = if size1 >= 3 || size2 >= 3 { 4 } else { 3 };

        // Decrement SP by total size
        let sp = self.registers.sp().wrapping_sub(total_size) & 0x00FF_FFFF;
        self.registers.set_sp(sp);

        // Write reg1 first (at lower address)
        for i in 0..size1 {
            let byte = ((value1 >> (i * 8)) & 0xFF) as u8;
            self.bus.write(Addr::new(sp + i), byte);
        }

        // Write reg2 on top (at higher address)
        for i in 0..size2 {
            let byte = ((value2 >> (i * 8)) & 0xFF) as u8;
            self.bus.write(Addr::new(sp + size1 + i), byte);
        }

        Ok(Cycles(cycles + odd_penalty(sp, size1) + odd_penalty(sp + size1, size2)))
    }

    /// POP2 reg1, reg2 — Pop two registers from stack (opcode 0x59, 3-4 cycles)
    /// Postbyte format: [reg1 4 bits][reg2 4 bits]
    /// Pop order: reg2 first (on top), then reg1 (lower in memory)
    /// This is the reverse of push order
    /// Cycle count: 3 if both are 8-bit or 16-bit, 4 if either is 24-bit or 32-bit
    pub(super) fn exec_pop2(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let reg1 = (postbyte >> 4) & 0x0F;
        let reg2 = postbyte & 0x0F;

        // Calculate sizes
        let size1 = Self::register_size(reg1);
        let size2 = Self::register_size(reg2);
        let total_size = size1 + size2;

        // Determine cycle count: 4 if either register is 24-bit or 32-bit, else 3
        let cycles = if size1 >= 3 || size2 >= 3 { 4 } else { 3 };

        let sp = self.registers.sp();

        // Read reg1 first (at lower address)
        let mut value1 = 0u32;
        for i in 0..size1 {
            let byte = self.bus.read(Addr::new(sp + i));
            value1 |= u32::from(byte) << (i * 8);
        }

        // Read reg2 (at higher address)
        let mut value2 = 0u32;
        for i in 0..size2 {
            let byte = self.bus.read(Addr::new(sp + size1 + i));
            value2 |= u32::from(byte) << (i * 8);
        }

        // Increment SP by total size
        self.registers.set_sp(sp + total_size);

        // Write both register values
        self.write_register(reg1, value1)?;
        self.write_register(reg2, value2)?;

        Ok(Cycles(cycles + odd_penalty(sp, size1) + odd_penalty(sp + size1, size2)))
    }

    // Extended Addressing Mode Handlers (0xC0-0xCD)

    /// LD A, $addr — Load A from extended address (opcode 0xC0, 3 cycles)
    pub(super) fn exec_ld_a_extended(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let addr = self.decode_extended(postbyte);
        let value = self.bus.read(Addr::new(addr));
        self.registers.set_a(value);
        self.update_nz_8(value);
        Ok(Cycles(3))
    }

    /// LD B, $addr — Load B from extended address (opcode 0xC1, 3 cycles)
    pub(super) fn exec_ld_b_extended(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let addr = self.decode_extended(postbyte);
        let value = self.bus.read(Addr::new(addr));
        self.registers.set_b(value);
        self.update_nz_8(value);
        Ok(Cycles(3))
    }

    /// LD D, $addr — Load D from extended address (opcode 0xC2, 3 cycles)
    pub(super) fn exec_ld_d_extended(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let addr = self.decode_extended(postbyte);
        let lsb = self.bus.read(Addr::new(addr));
        let msb = self.bus.read(Addr::new(addr + 1));
        let value = u16::from(lsb) | (u16::from(msb) << 8);
        self.registers.set_d(value);
        self.update_nz_16(value);
        Ok(Cycles(3 + odd_penalty(addr, 2)))
    }

    /// LD E, $addr — Load E from extended address (opcode 0xC3, 3 cycles)
    pub(super) fn exec_ld_e_extended(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let addr = self.decode_extended(postbyte);
        let value = self.bus.read(Addr::new(addr));
        self.registers.set_e(value);
        self.update_nz_8(value);
        Ok(Cycles(3))
    }

    /// LD F, $addr — Load F from extended address (opcode 0xC4, 3 cycles)
    pub(super) fn exec_ld_f_extended(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let addr = self.decode_extended(postbyte);
        let value = self.bus.read(Addr::new(addr));
        self.registers.set_f(value);
        self.update_nz_8(value);
        Ok(Cycles(3))
    }

    /// LD W, $addr — Load W from extended address (opcode 0xC5, 3 cycles)
    pub(super) fn exec_ld_w_extended(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let addr = self.decode_extended(postbyte);
        let lsb = self.bus.read(Addr::new(addr));
        let msb = self.bus.read(Addr::new(addr + 1));
        let value = u16::from(lsb) | (u16::from(msb) << 8);
        self.registers.set_w(value);
        self.update_nz_16(value);
        Ok(Cycles(3 + odd_penalty(addr, 2)))
    }

    /// LD Q, $addr — Load Q from extended address (opcode 0xC6, 4 cycles)
    pub(super) fn exec_ld_q_extended(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let addr = self.decode_extended(postbyte);
        let byte0 = self.bus.read(Addr::new(addr));
        let byte1 = self.bus.read(Addr::new(addr + 1));
        let byte2 = self.bus.read(Addr::new(addr + 2));
        let byte3 = self.bus.read(Addr::new(addr + 3));
        let value = u32::from(byte0) | (u32::from(byte1) << 8) | (u32::from(byte2) << 16) | (u32::from(byte3) << 24);
        self.registers.set_q(value);
        self.update_nz_32(value);
        Ok(Cycles(4 + odd_penalty(addr, 4)))
    }

    /// ST A, $addr — Store A to extended address (opcode 0xC7, 3 cycles)
    pub(super) fn exec_st_a_extended(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let addr = self.decode_extended(postbyte);
        let value = self.registers.a();
        self.bus.write(Addr::new(addr), value);
        Ok(Cycles(3))
    }

    /// ST B, $addr — Store B to extended address (opcode 0xC8, 3 cycles)
    pub(super) fn exec_st_b_extended(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let addr = self.decode_extended(postbyte);
        let value = self.registers.b();
        self.bus.write(Addr::new(addr), value);
        Ok(Cycles(3))
    }

    /// ST D, $addr — Store D to extended address (opcode 0xC9, 3 cycles)
    pub(super) fn exec_st_d_extended(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let addr = self.decode_extended(postbyte);
        let value = self.registers.d();
        // Little-endian: LSB at lower address
        self.bus.write(Addr::new(addr), value as u8);
        self.bus.write(Addr::new(addr + 1), (value >> 8) as u8);
        Ok(Cycles(3 + odd_penalty(addr, 2)))
    }

    /// ST E, $addr — Store E to extended address (opcode 0xCA, 3 cycles)
    pub(super) fn exec_st_e_extended(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let addr = self.decode_extended(postbyte);
        let value = self.registers.e();
        self.bus.write(Addr::new(addr), value);
        Ok(Cycles(3))
    }

    /// ST F, $addr — Store F to extended address (opcode 0xCB, 3 cycles)
    pub(super) fn exec_st_f_extended(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let addr = self.decode_extended(postbyte);
        let value = self.registers.f();
        self.bus.write(Addr::new(addr), value);
        Ok(Cycles(3))
    }

    /// ST W, $addr — Store W to extended address (opcode 0xCC, 3 cycles)
    pub(super) fn exec_st_w_extended(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let addr = self.decode_extended(postbyte);
        let value = self.registers.w();
        // Little-endian: LSB at lower address
        self.bus.write(Addr::new(addr), value as u8);
        self.bus.write(Addr::new(addr + 1), (value >> 8) as u8);
        Ok(Cycles(3 + odd_penalty(addr, 2)))
    }

    /// ST Q, $addr — Store Q to extended address (opcode 0xCD, 4 cycles)
    pub(super) fn exec_st_q_extended(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let addr = self.decode_extended(postbyte);
        let value = self.registers.q();
        // Little-endian: LSB at lower address
        self.bus.write(Addr::new(addr), value as u8);
        self.bus.write(Addr::new(addr + 1), (value >> 8) as u8);
        self.bus.write(Addr::new(addr + 2), (value >> 16) as u8);
        self.bus.write(Addr::new(addr + 3), (value >> 24) as u8);
        Ok(Cycles(4 + odd_penalty(addr, 4)))
    }

    // Indexed Addressing Mode Handlers (0xD0-0xDD)

    /// LD A, indexed — Load A indexed (opcode 0xD0, 3+ cycles)
    pub(super) fn exec_ld_a_indexed(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if let Some((addr, extra_cycles)) = self.decode_indexed(postbyte) {
            let value = self.bus.read(Addr::new(addr));
            self.registers.set_a(value);
            self.update_nz_8(value);
            Ok(Cycles(3 + extra_cycles))
        } else {
            // Illegal indexed postbyte - execute as 1-cycle NOP
            Ok(Cycles(1))
        }
    }

    /// LD B, indexed — Load B indexed (opcode 0xD1, 3+ cycles)
    pub(super) fn exec_ld_b_indexed(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if let Some((addr, extra_cycles)) = self.decode_indexed(postbyte) {
            let value = self.bus.read(Addr::new(addr));
            self.registers.set_b(value);
            self.update_nz_8(value);
            Ok(Cycles(3 + extra_cycles))
        } else {
            // Illegal indexed postbyte - execute as 1-cycle NOP
            Ok(Cycles(1))
        }
    }

    /// LD D, indexed — Load D indexed (opcode 0xD2, 3+ cycles)
    pub(super) fn exec_ld_d_indexed(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if let Some((addr, extra_cycles)) = self.decode_indexed(postbyte) {
            let lsb = self.bus.read(Addr::new(addr));
            let msb = self.bus.read(Addr::new(addr + 1));
            let value = u16::from(lsb) | (u16::from(msb) << 8);
            self.registers.set_d(value);
            self.update_nz_16(value);
            Ok(Cycles(3 + extra_cycles + odd_penalty(addr, 2)))
        } else {
            // Illegal indexed postbyte - execute as 1-cycle NOP
            Ok(Cycles(1))
        }
    }

    /// LD E, indexed — Load E indexed (opcode 0xD3, 3+ cycles)
    pub(super) fn exec_ld_e_indexed(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if let Some((addr, extra_cycles)) = self.decode_indexed(postbyte) {
            let value = self.bus.read(Addr::new(addr));
            self.registers.set_e(value);
            self.update_nz_8(value);
            Ok(Cycles(3 + extra_cycles))
        } else {
            // Illegal indexed postbyte - execute as 1-cycle NOP
            Ok(Cycles(1))
        }
    }

    /// LD F, indexed — Load F indexed (opcode 0xD4, 3+ cycles)
    pub(super) fn exec_ld_f_indexed(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if let Some((addr, extra_cycles)) = self.decode_indexed(postbyte) {
            let value = self.bus.read(Addr::new(addr));
            self.registers.set_f(value);
            self.update_nz_8(value);
            Ok(Cycles(3 + extra_cycles))
        } else {
            // Illegal indexed postbyte - execute as 1-cycle NOP
            Ok(Cycles(1))
        }
    }

    /// LD W, indexed — Load W indexed (opcode 0xD5, 3+ cycles)
    pub(super) fn exec_ld_w_indexed(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if let Some((addr, extra_cycles)) = self.decode_indexed(postbyte) {
            let lsb = self.bus.read(Addr::new(addr));
            let msb = self.bus.read(Addr::new(addr + 1));
            let value = u16::from(lsb) | (u16::from(msb) << 8);
            self.registers.set_w(value);
            self.update_nz_16(value);
            Ok(Cycles(3 + extra_cycles + odd_penalty(addr, 2)))
        } else {
            // Illegal indexed postbyte - execute as 1-cycle NOP
            Ok(Cycles(1))
        }
    }

    /// LD Q, indexed — Load Q indexed (opcode 0xD6, 4+ cycles)
    pub(super) fn exec_ld_q_indexed(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if let Some((addr, extra_cycles)) = self.decode_indexed(postbyte) {
            let byte0 = self.bus.read(Addr::new(addr));
            let byte1 = self.bus.read(Addr::new(addr + 1));
            let byte2 = self.bus.read(Addr::new(addr + 2));
            let byte3 = self.bus.read(Addr::new(addr + 3));
            let value = u32::from(byte0) | (u32::from(byte1) << 8) | (u32::from(byte2) << 16) | (u32::from(byte3) << 24);
            self.registers.set_q(value);
            self.update_nz_32(value);
            Ok(Cycles(4 + extra_cycles + odd_penalty(addr, 4)))
        } else {
            // Illegal indexed postbyte - execute as 1-cycle NOP
            Ok(Cycles(1))
        }
    }

    /// ST A, indexed — Store A indexed (opcode 0xD7, 3+ cycles)
    pub(super) fn exec_st_a_indexed(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if let Some((addr, extra_cycles)) = self.decode_indexed(postbyte) {
            let value = self.registers.a();
            self.bus.write(Addr::new(addr), value);
            Ok(Cycles(3 + extra_cycles))
        } else {
            // Illegal indexed postbyte - execute as 1-cycle NOP
            Ok(Cycles(1))
        }
    }

    /// ST B, indexed — Store B indexed (opcode 0xD8, 3+ cycles)
    pub(super) fn exec_st_b_indexed(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if let Some((addr, extra_cycles)) = self.decode_indexed(postbyte) {
            let value = self.registers.b();
            self.bus.write(Addr::new(addr), value);
            Ok(Cycles(3 + extra_cycles))
        } else {
            // Illegal indexed postbyte - execute as 1-cycle NOP
            Ok(Cycles(1))
        }
    }

    /// ST D, indexed — Store D indexed (opcode 0xD9, 3+ cycles)
    pub(super) fn exec_st_d_indexed(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if let Some((addr, extra_cycles)) = self.decode_indexed(postbyte) {
            let value = self.registers.d();
            // Little-endian: LSB at lower address
            self.bus.write(Addr::new(addr), value as u8);
            self.bus.write(Addr::new(addr + 1), (value >> 8) as u8);
            Ok(Cycles(3 + extra_cycles + odd_penalty(addr, 2)))
        } else {
            // Illegal indexed postbyte - execute as 1-cycle NOP
            Ok(Cycles(1))
        }
    }

    /// ST E, indexed — Store E indexed (opcode 0xDA, 3+ cycles)
    pub(super) fn exec_st_e_indexed(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if let Some((addr, extra_cycles)) = self.decode_indexed(postbyte) {
            let value = self.registers.e();
            self.bus.write(Addr::new(addr), value);
            Ok(Cycles(3 + extra_cycles))
        } else {
            // Illegal indexed postbyte - execute as 1-cycle NOP
            Ok(Cycles(1))
        }
    }

    /// ST F, indexed — Store F indexed (opcode 0xDB, 3+ cycles)
    pub(super) fn exec_st_f_indexed(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if let Some((addr, extra_cycles)) = self.decode_indexed(postbyte) {
            let value = self.registers.f();
            self.bus.write(Addr::new(addr), value);
            Ok(Cycles(3 + extra_cycles))
        } else {
            // Illegal indexed postbyte - execute as 1-cycle NOP
            Ok(Cycles(1))
        }
    }

    /// ST W, indexed — Store W indexed (opcode 0xDC, 3+ cycles)
    pub(super) fn exec_st_w_indexed(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if let Some((addr, extra_cycles)) = self.decode_indexed(postbyte) {
            let value = self.registers.w();
            // Little-endian: LSB at lower address
            self.bus.write(Addr::new(addr), value as u8);
            self.bus.write(Addr::new(addr + 1), (value >> 8) as u8);
            Ok(Cycles(3 + extra_cycles + odd_penalty(addr, 2)))
        } else {
            // Illegal indexed postbyte - execute as 1-cycle NOP
            Ok(Cycles(1))
        }
    }

    /// ST Q, indexed — Store Q indexed (opcode 0xDD, 4+ cycles)
    pub(super) fn exec_st_q_indexed(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if let Some((addr, extra_cycles)) = self.decode_indexed(postbyte) {
            let value = self.registers.q();
            // Little-endian: LSB at lower address
            self.bus.write(Addr::new(addr), value as u8);
            self.bus.write(Addr::new(addr + 1), (value >> 8) as u8);
            self.bus.write(Addr::new(addr + 2), (value >> 16) as u8);
            self.bus.write(Addr::new(addr + 3), (value >> 24) as u8);
            Ok(Cycles(4 + extra_cycles + odd_penalty(addr, 4)))
        } else {
            // Illegal indexed postbyte - execute as 1-cycle NOP
            Ok(Cycles(1))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Ram;

    #[test]
    fn test_ld_a_imm() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // Load immediate value into A
        let result = interp.exec_ld_a_imm(0x42);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(2));
        assert_eq!(interp.registers.a(), 0x42);

        // Check flags: N=0 (not negative), Z=0 (not zero)
        assert!(!interp.registers.cc().contains(Flags::N));
        assert!(!interp.registers.cc().contains(Flags::Z));
    }

    #[test]
    fn test_ld_a_imm_zero() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        let result = interp.exec_ld_a_imm(0x00);
        assert!(result.is_ok());
        assert_eq!(interp.registers.a(), 0x00);

        // Check flags: N=0, Z=1 (zero result)
        assert!(!interp.registers.cc().contains(Flags::N));
        assert!(interp.registers.cc().contains(Flags::Z));
    }

    #[test]
    fn test_ld_a_imm_negative() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        let result = interp.exec_ld_a_imm(0x80);
        assert!(result.is_ok());
        assert_eq!(interp.registers.a(), 0x80);

        // Check flags: N=1 (bit 7 set), Z=0
        assert!(interp.registers.cc().contains(Flags::N));
        assert!(!interp.registers.cc().contains(Flags::Z));
    }

    #[test]
    fn test_mov_reg_reg() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // Set up source register A with a value
        interp.registers.set_a(0x55);

        // MOV B, A (postbyte: src=0, dest=1)
        let postbyte = 1;
        let result = interp.exec_mov(postbyte);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(1));
        assert_eq!(interp.registers.b(), 0x55);

        // Check flags updated for destination
        assert!(!interp.registers.cc().contains(Flags::N));
        assert!(!interp.registers.cc().contains(Flags::Z));
    }

    #[test]
    fn test_st_a() {
        let mut ram = Ram::new(1024);

        // Write extended address to memory (little-endian): $000100
        ram.write(Addr::new(0), 0x00); // mid byte (will be read at PC=0)
        ram.write(Addr::new(1), 0x01); // MSB (will be read at PC=1)

        let mut interp = ReferenceInterpreter::new(ram);

        // Set up register
        interp.registers.set_a(0x99);
        interp.registers.set_pc(0); // PC will be read from for extended address

        // Store A to extended address (postbyte=0x00 is LSB of address)
        let result = interp.exec_st_a(0x00);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(3));

        assert_eq!(interp.bus.read(Addr::new(0x0001_0000)), 0x99);
    }

    #[test]
    fn test_ld_d_imm() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // Prepare postbyte (LSB) and next byte (MSB) in memory
        interp.registers.set_pc(0);
        let mut ram = Ram::new(1024);
        ram.write(Addr::new(0), 0x78); // MSB
        interp.bus = ram;

        // LD D, #$5678
        let result = interp.exec_ld_d_imm(0x56); // postbyte is LSB
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(2));
        assert_eq!(interp.registers.d(), 0x7856);
        assert_eq!(interp.registers.a(), 0x78);
        assert_eq!(interp.registers.b(), 0x56);
    }

    #[test]
    fn test_ld_q_imm() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // Prepare 4-byte immediate in memory (little-endian)
        interp.registers.set_pc(0);
        let mut ram = Ram::new(1024);
        ram.write(Addr::new(0), 0x34); // byte 1
        ram.write(Addr::new(1), 0x56); // byte 2
        ram.write(Addr::new(2), 0x78); // byte 3
        interp.bus = ram;

        // LD Q, #$78563412 (postbyte=0x12)
        let result = interp.exec_ld_q_imm(0x12);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(3));
        assert_eq!(interp.registers.q(), 0x7856_3412);
    }

    #[test]
    fn test_st_d_little_endian() {
        let mut ram = Ram::new(1024);

        // Prepare extended address in memory (address will be 0x0002_0000)
        ram.write(Addr::new(0), 0x00); // addr byte 1 (will be read at PC=0)
        ram.write(Addr::new(1), 0x00); // addr byte 2 (will be read at PC=1)
        ram.write(Addr::new(2), 0x02); // This won't be used in this test

        let mut interp = ReferenceInterpreter::new(ram);
        interp.registers.set_d(0xABCD);
        interp.registers.set_pc(0);

        // ST D, address (postbyte=0x00 is LSB, bytes at PC+0 and PC+1 complete address)
        // Address will be: 0x00 | 0x00<<8 | 0x02<<16 = 0x0002_0000
        interp.bus.write(Addr::new(1), 0x02);

        let result = interp.exec_st_d(0x00);
        assert!(result.is_ok());

        assert_eq!(interp.bus.read(Addr::new(0x0002_0000)), 0xCD); // LSB
        assert_eq!(interp.bus.read(Addr::new(0x0002_0001)), 0xAB); // MSB
    }

    #[test]
    fn test_exg_d_w() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // Set up registers
        interp.registers.set_d(0x1234);
        interp.registers.set_w(0x5678);

        // EXG D, W (postbyte: src=0x2 (D), dest=0x5 (W))
        let postbyte = (0x2 << 4) | 0x5;
        let result = interp.exec_exg(postbyte);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(1));

        assert_eq!(interp.registers.d(), 0x5678);
        assert_eq!(interp.registers.w(), 0x1234);
    }

    #[test]
    fn test_push_pop_8bit() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // Set initial SP and register value
        interp.registers.set_sp(0x1000);
        interp.registers.set_a(0x42);

        // PUSH A (postbyte encodes reg=0)
        let result = interp.exec_push(0x00);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(2));
        assert_eq!(interp.registers.sp(), 0x0FFF); // SP decremented by 1
        assert_eq!(interp.bus.read(Addr::new(0x0FFF)), 0x42);

        // Clear A
        interp.registers.set_a(0x00);

        // POP A
        let result = interp.exec_pop(0x00);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(2));
        assert_eq!(interp.registers.sp(), 0x1000); // SP restored
        assert_eq!(interp.registers.a(), 0x42); // A restored
    }

    #[test]
    fn test_push_pop_cc_and_dp() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));
        interp.registers.set_sp(0x1000);
        interp.registers.set_cc(Flags::N | Flags::C);
        interp.registers.set_dp(0x12);

        assert_eq!(interp.exec_push(0x0D).unwrap(), Cycles(2)); // PUSH CC
        assert_eq!(interp.exec_push(0x0C).unwrap(), Cycles(2)); // PUSH DP
        assert_eq!(interp.registers.sp(), 0x0FFE);
        assert_eq!(interp.bus.read(Addr::new(0x0FFF)), (Flags::N | Flags::C).bits());
        assert_eq!(interp.bus.read(Addr::new(0x0FFE)), 0x12);

        interp.registers.set_cc(Flags::Z);
        interp.registers.set_dp(0);
        assert_eq!(interp.exec_pop(0x0C).unwrap(), Cycles(2)); // POP DP
        assert_eq!(interp.exec_pop(0x0D).unwrap(), Cycles(2)); // POP CC
        assert_eq!(interp.registers.dp(), 0x12);
        assert_eq!(interp.registers.cc(), Flags::N | Flags::C);
        assert_eq!(interp.registers.sp(), 0x1000);
        assert!(interp.exec_push(0x0B).is_err(), "PUSH PC stays illegal");
    }

    #[test]
    fn test_push_pop_16bit() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // Set initial SP and register value
        interp.registers.set_sp(0x1000);
        interp.registers.set_d(0xABCD);

        // PUSH D (postbyte encodes reg=2)
        let result = interp.exec_push(0x02);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(2));
        assert_eq!(interp.registers.sp(), 0x0FFE); // SP decremented by 2

        assert_eq!(interp.bus.read(Addr::new(0x0FFE)), 0xCD); // LSB
        assert_eq!(interp.bus.read(Addr::new(0x0FFF)), 0xAB); // MSB

        // Clear D
        interp.registers.set_d(0x0000);

        // POP D
        let result = interp.exec_pop(0x02);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(2));
        assert_eq!(interp.registers.sp(), 0x1000); // SP restored
        assert_eq!(interp.registers.d(), 0xABCD); // D restored
    }

    #[test]
    fn test_push_pop_32bit() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // Set initial SP and register value
        interp.registers.set_sp(0x1000);
        interp.registers.set_q(0x1234_5678);

        // PUSH Q (postbyte encodes reg=6)
        let result = interp.exec_push(0x06);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(3));
        assert_eq!(interp.registers.sp(), 0x0FFC); // SP decremented by 4

        assert_eq!(interp.bus.read(Addr::new(0x0FFC)), 0x78); // LSB
        assert_eq!(interp.bus.read(Addr::new(0x0FFD)), 0x56);
        assert_eq!(interp.bus.read(Addr::new(0x0FFE)), 0x34);
        assert_eq!(interp.bus.read(Addr::new(0x0FFF)), 0x12); // MSB

        // Clear Q
        interp.registers.set_q(0x0000_0000);

        // POP Q
        let result = interp.exec_pop(0x06);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(3));
        assert_eq!(interp.registers.sp(), 0x1000); // SP restored
        assert_eq!(interp.registers.q(), 0x1234_5678); // Q restored
    }

    #[test]
    fn test_push_pop_24bit() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // Set initial SP and register value
        interp.registers.set_sp(0x1000);
        interp.registers.set_x(0x0012_3456);

        // PUSH X (postbyte encodes reg=8)
        let result = interp.exec_push(0x08);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(3));
        assert_eq!(interp.registers.sp(), 0x0FFD); // SP decremented by 3

        assert_eq!(interp.bus.read(Addr::new(0x0FFD)), 0x56); // LSB
        assert_eq!(interp.bus.read(Addr::new(0x0FFE)), 0x34);
        assert_eq!(interp.bus.read(Addr::new(0x0FFF)), 0x12); // MSB

        // Clear X
        interp.registers.set_x(0x0000_0000);

        // POP X
        let result = interp.exec_pop(0x08);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(3));
        assert_eq!(interp.registers.sp(), 0x1000); // SP restored
        assert_eq!(interp.registers.x(), 0x0012_3456); // X restored
    }

    #[test]
    fn test_mov_with_truncation() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // Set up 16-bit register
        interp.registers.set_d(0xABCD);

        // MOV A, D (postbyte: src=2 (D), dest=0 (A))
        // Should truncate to 8-bit
        let postbyte = 0x2 << 4;
        let result = interp.exec_mov(postbyte);
        assert!(result.is_ok());
        assert_eq!(interp.registers.a(), 0xCD); // LSB only
    }

    #[test]
    fn test_mov_to_pointer_and_dp_sets_nz() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));
        interp.registers.set_d(0);
        interp.exec_mov(0x28).unwrap(); // MOV X,D
        assert!(interp.registers.cc().contains(Flags::Z));
        interp.registers.set_q(0x0080_0000);
        interp.exec_mov(0x69).unwrap(); // MOV Y,Q: bit 23 is the sign
        assert!(interp.registers.cc().contains(Flags::N));
        assert!(!interp.registers.cc().contains(Flags::Z));
        interp.registers.set_a(0x80);
        interp.exec_mov(0x0C).unwrap(); // MOV DP,A
        assert!(interp.registers.cc().contains(Flags::N));
    }

    #[test]
    fn test_mov_with_extension() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // Set up 8-bit register
        interp.registers.set_a(0x42);

        // MOV D, A (postbyte: src=0 (A), dest=2 (D))
        // Should zero-extend to 16-bit
        let postbyte = 0x2;
        let result = interp.exec_mov(postbyte);
        assert!(result.is_ok());
        assert_eq!(interp.registers.d(), 0x0042); // Zero-extended
    }

    #[test]
    fn test_push2_pop2_8bit_8bit() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // Set initial SP and register values
        interp.registers.set_sp(0x1000);
        interp.registers.set_a(0x42);
        interp.registers.set_b(0x99);

        // PUSH2 A, B (postbyte: reg1=0 (A), reg2=1 (B))
        let postbyte = 0x1;
        let result = interp.exec_push2(postbyte);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(3)); // 8-bit + 8-bit = 3 cycles
        assert_eq!(interp.registers.sp(), 0x0FFE); // SP decremented by 2

        assert_eq!(interp.bus.read(Addr::new(0x0FFE)), 0x42); // A at lower address
        assert_eq!(interp.bus.read(Addr::new(0x0FFF)), 0x99); // B on top

        // Clear registers
        interp.registers.set_a(0x00);
        interp.registers.set_b(0x00);

        // POP2 A, B
        let result = interp.exec_pop2(postbyte);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(3)); // 8-bit + 8-bit = 3 cycles
        assert_eq!(interp.registers.sp(), 0x1000); // SP restored
        assert_eq!(interp.registers.a(), 0x42); // A restored
        assert_eq!(interp.registers.b(), 0x99); // B restored
    }

    #[test]
    fn test_push2_pop2_8bit_16bit() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // Set initial SP and register values
        interp.registers.set_sp(0x1000);
        interp.registers.set_d(0x1234);
        interp.registers.set_a(0xAA);

        // Now A = 0xAA, D = 0xAA34 (because A is high byte of D)
        interp.registers.set_e(0xBB);
        interp.registers.set_d(0x1234);

        // PUSH2 E, D (postbyte: reg1=3 (E), reg2=2 (D))
        let postbyte = (0x3 << 4) | 0x2;
        let result = interp.exec_push2(postbyte);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(3)); // 8-bit + 16-bit = 3 cycles
        assert_eq!(interp.registers.sp(), 0x0FFD); // SP decremented by 3

        assert_eq!(interp.bus.read(Addr::new(0x0FFD)), 0xBB); // E at lowest address
        assert_eq!(interp.bus.read(Addr::new(0x0FFE)), 0x34); // D LSB
        assert_eq!(interp.bus.read(Addr::new(0x0FFF)), 0x12); // D MSB on top

        // Clear registers
        interp.registers.set_e(0x00);
        interp.registers.set_d(0x0000);

        // POP2 E, D
        let result = interp.exec_pop2(postbyte);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(3)); // 8-bit + 16-bit = 3 cycles
        assert_eq!(interp.registers.sp(), 0x1000); // SP restored
        assert_eq!(interp.registers.e(), 0xBB); // E restored
        assert_eq!(interp.registers.d(), 0x1234); // D restored
    }

    #[test]
    fn test_push2_pop2_16bit_16bit() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // Set initial SP and register values
        interp.registers.set_sp(0x1000);
        interp.registers.set_d(0xABCD);
        interp.registers.set_w(0x5678);

        // PUSH2 D, W (postbyte: reg1=2 (D), reg2=5 (W))
        let postbyte = (0x2 << 4) | 0x5;
        let result = interp.exec_push2(postbyte);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(3)); // 16-bit + 16-bit = 3 cycles
        assert_eq!(interp.registers.sp(), 0x0FFC); // SP decremented by 4

        assert_eq!(interp.bus.read(Addr::new(0x0FFC)), 0xCD); // D LSB
        assert_eq!(interp.bus.read(Addr::new(0x0FFD)), 0xAB); // D MSB
        assert_eq!(interp.bus.read(Addr::new(0x0FFE)), 0x78); // W LSB
        assert_eq!(interp.bus.read(Addr::new(0x0FFF)), 0x56); // W MSB on top

        // Clear registers
        interp.registers.set_d(0x0000);
        interp.registers.set_w(0x0000);

        // POP2 D, W
        let result = interp.exec_pop2(postbyte);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(3)); // 16-bit + 16-bit = 3 cycles
        assert_eq!(interp.registers.sp(), 0x1000); // SP restored
        assert_eq!(interp.registers.d(), 0xABCD); // D restored
        assert_eq!(interp.registers.w(), 0x5678); // W restored
    }

    #[test]
    fn test_push2_pop2_24bit() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // Set initial SP and register values
        interp.registers.set_sp(0x1000);
        interp.registers.set_x(0x0012_3456);
        interp.registers.set_y(0x00AB_CDEF);

        // PUSH2 X, Y (postbyte: reg1=8 (X), reg2=9 (Y))
        let postbyte = (0x8 << 4) | 0x9;
        let result = interp.exec_push2(postbyte);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(4)); // 24-bit + 24-bit = 4 cycles
        assert_eq!(interp.registers.sp(), 0x0FFA); // SP decremented by 6

        assert_eq!(interp.bus.read(Addr::new(0x0FFA)), 0x56); // X byte 0
        assert_eq!(interp.bus.read(Addr::new(0x0FFB)), 0x34); // X byte 1
        assert_eq!(interp.bus.read(Addr::new(0x0FFC)), 0x12); // X byte 2
        assert_eq!(interp.bus.read(Addr::new(0x0FFD)), 0xEF); // Y byte 0
        assert_eq!(interp.bus.read(Addr::new(0x0FFE)), 0xCD); // Y byte 1
        assert_eq!(interp.bus.read(Addr::new(0x0FFF)), 0xAB); // Y byte 2 on top

        // Clear registers
        interp.registers.set_x(0x0000_0000);
        interp.registers.set_y(0x0000_0000);

        // POP2 X, Y
        let result = interp.exec_pop2(postbyte);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(4)); // 24-bit + 24-bit = 4 cycles
        assert_eq!(interp.registers.sp(), 0x1000); // SP restored
        assert_eq!(interp.registers.x(), 0x0012_3456); // X restored
        assert_eq!(interp.registers.y(), 0x00AB_CDEF); // Y restored
    }

    #[test]
    fn test_push2_pop2_32bit() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // Set initial SP and register values
        interp.registers.set_sp(0x1000);
        interp.registers.set_q(0x1234_5678);
        // Use GP which doesn't overlap with Q
        interp.registers.set_gp(0xCC);

        // PUSH2 Q, GP (postbyte: reg1=6 (Q), reg2=7 (GP))
        let postbyte = (0x6 << 4) | 0x7;
        let result = interp.exec_push2(postbyte);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(5)); // 4, +1 because Q lands at an odd address
        assert_eq!(interp.registers.sp(), 0x0FFB); // SP decremented by 5

        assert_eq!(interp.bus.read(Addr::new(0x0FFB)), 0x78); // Q byte 0
        assert_eq!(interp.bus.read(Addr::new(0x0FFC)), 0x56); // Q byte 1
        assert_eq!(interp.bus.read(Addr::new(0x0FFD)), 0x34); // Q byte 2
        assert_eq!(interp.bus.read(Addr::new(0x0FFE)), 0x12); // Q byte 3
        assert_eq!(interp.bus.read(Addr::new(0x0FFF)), 0xCC); // GP on top

        // Clear registers
        interp.registers.set_q(0x0000_0000);
        interp.registers.set_gp(0x00);

        // POP2 Q, GP
        let result = interp.exec_pop2(postbyte);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(5)); // 4, +1 for Q at an odd address
        assert_eq!(interp.registers.sp(), 0x1000); // SP restored
        assert_eq!(interp.registers.q(), 0x1234_5678); // Q restored
        assert_eq!(interp.registers.gp(), 0xCC); // GP restored
    }

    #[test]
    fn test_push2_pop2_mixed_8bit_24bit() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // Set initial SP and register values
        interp.registers.set_sp(0x1000);
        interp.registers.set_b(0x55);
        interp.registers.set_x(0x00FE_DCBA);

        // PUSH2 B, X (postbyte: reg1=1 (B), reg2=8 (X))
        let postbyte = (0x1 << 4) | 0x8;
        let result = interp.exec_push2(postbyte);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(4)); // 8-bit + 24-bit = 4 cycles
        assert_eq!(interp.registers.sp(), 0x0FFC); // SP decremented by 4

        assert_eq!(interp.bus.read(Addr::new(0x0FFC)), 0x55); // B at lowest
        assert_eq!(interp.bus.read(Addr::new(0x0FFD)), 0xBA); // X byte 0
        assert_eq!(interp.bus.read(Addr::new(0x0FFE)), 0xDC); // X byte 1
        assert_eq!(interp.bus.read(Addr::new(0x0FFF)), 0xFE); // X byte 2 on top

        // Clear registers
        interp.registers.set_b(0x00);
        interp.registers.set_x(0x0000_0000);

        // POP2 B, X
        let result = interp.exec_pop2(postbyte);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(4)); // 8-bit + 24-bit = 4 cycles
        assert_eq!(interp.registers.sp(), 0x1000); // SP restored
        assert_eq!(interp.registers.b(), 0x55); // B restored
        assert_eq!(interp.registers.x(), 0x00FE_DCBA); // X restored
    }

    #[test]
    fn test_push2_stack_ordering() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // Test that reg2 ends up on top of stack (higher address)
        interp.registers.set_sp(0x1000);
        interp.registers.set_a(0x11);
        interp.registers.set_b(0x22);

        // PUSH2 A, B
        let postbyte = 0x1;
        let _result = interp.exec_push2(postbyte);

        // A should be at lower address, B on top
        assert_eq!(interp.bus.read(Addr::new(0x0FFE)), 0x11); // A lower
        assert_eq!(interp.bus.read(Addr::new(0x0FFF)), 0x22); // B higher (on top)

        interp.registers.set_e(0x33);
        let _result = interp.exec_push(0x03); // PUSH E
        assert_eq!(interp.registers.sp(), 0x0FFD);
        assert_eq!(interp.bus.read(Addr::new(0x0FFD)), 0x33); // E now lowest
        assert_eq!(interp.bus.read(Addr::new(0x0FFE)), 0x11); // A middle
        assert_eq!(interp.bus.read(Addr::new(0x0FFF)), 0x22); // B still on top
    }

    #[test]
    fn test_push2_pop2_no_flags_affected() {
        use crate::Flags;
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // Set flags to a known state
        interp.registers.set_cc(Flags::N | Flags::Z | Flags::C);
        let initial_flags = interp.registers.cc();

        // Set up for PUSH2/POP2
        interp.registers.set_sp(0x1000);
        interp.registers.set_d(0xABCD);
        interp.registers.set_w(0x5678);

        // PUSH2 D, W
        let postbyte = (0x2 << 4) | 0x5;
        let _result = interp.exec_push2(postbyte);

        // Flags should be unchanged
        assert_eq!(interp.registers.cc(), initial_flags);

        // POP2 D, W
        let _result = interp.exec_pop2(postbyte);

        // Flags should still be unchanged
        assert_eq!(interp.registers.cc(), initial_flags);
    }
}
