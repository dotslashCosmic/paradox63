#![allow(dead_code)]

pub const SPEC_VERSION: &str = "0.11.0";
pub const CLOCK_HZ: u32 = 7159090;
pub const AUDIO_SAMPLE_DIV: u32 = 224;
pub const DOTS_PER_LINE: u32 = 455;
pub const LINES_PER_FRAME: u32 = 262;
pub const DP_WINDOW_BYTES: u32 = 64;
pub const LOW_VECTOR_ROM: [u8; 4] = [0x00, 0x00, 0xB6, 0xFB];
pub const ENV_STEPS: [u16; 16] = [0, 1, 2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64, 128, 256, 65535];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backing { Ram, Rom, Io }

pub struct Region { pub id: &'static str, pub name: &'static str, pub base: u32, pub end: u32, pub backing: Backing }

pub const REGIONS: [Region; 14] = [
    Region { id: "vec_lo", name: "Low Vectors", base: 0x000000, end: 0x000003, backing: Backing::Rom },
    Region { id: "dp_area", name: "Direct Page", base: 0x000004, end: 0x0000FF, backing: Backing::Ram },
    Region { id: "main", name: "Program, data, stack", base: 0x000100, end: 0xFBB5FF, backing: Backing::Ram },
    Region { id: "boot_rom", name: "Boot ROM", base: 0xFBB600, end: 0xFBF5FF, backing: Backing::Rom },
    Region { id: "cart_win", name: "Cart Window", base: 0xFBF600, end: 0xFCF5FF, backing: Backing::Rom },
    Region { id: "aram", name: "Audio RAM", base: 0xFCF600, end: 0xFDF5FF, backing: Backing::Ram },
    Region { id: "oam", name: "Sprite OAM", base: 0xFDF600, end: 0xFDFDFF, backing: Backing::Ram },
    Region { id: "vram", name: "VRAM Map", base: 0xFDFE00, end: 0xFFFDFF, backing: Backing::Ram },
    Region { id: "io_resv", name: "Reserved I/O", base: 0xFFFE00, end: 0xFFFEEF, backing: Backing::Io },
    Region { id: "cart_io", name: "Cart I/O", base: 0xFFFEF0, end: 0xFFFEFF, backing: Backing::Io },
    Region { id: "audio_io", name: "Audio I/O", base: 0xFFFF00, end: 0xFFFF7F, backing: Backing::Io },
    Region { id: "video_io", name: "Video I/O", base: 0xFFFF80, end: 0xFFFFBF, backing: Backing::Io },
    Region { id: "kb_io", name: "I/O Devices", base: 0xFFFFC0, end: 0xFFFFDF, backing: Backing::Io },
    Region { id: "vec_hi", name: "Interrupts", base: 0xFFFFE0, end: 0xFFFFFF, backing: Backing::Ram },
];

// constants
pub const SP_RESET: u32 = 0xFBB5FF;
pub const MAIN_BASE: u32 = 0x000100;
pub const MAIN_TOP: u32 = 0xFBB5FF;
pub const VRAM_BASE: u32 = 0xFDFE00;
pub const VRAM_END: u32 = 0xFFFDFF;
pub const OAM_BASE: u32 = 0xFDF600;
pub const OAM_END: u32 = 0xFDFDFF;
pub const ARAM_BASE: u32 = 0xFCF600;
pub const ARAM_END: u32 = 0xFDF5FF;
pub const AUDIO_IO_BASE: u32 = 0xFFFF00;
pub const AUDIO_IO_END: u32 = 0xFFFF7F;
pub const VIDEO_IO_BASE: u32 = 0xFFFF80;
pub const VIDEO_IO_END: u32 = 0xFFFFBF;
pub const KB_IO_BASE: u32 = 0xFFFFC0;
pub const KB_IO_END: u32 = 0xFFFFDF;
pub const VEC_SWI: u32 = 0xFFFFF5;
pub const VEC_NMI: u32 = 0xFFFFF8;
pub const VEC_IRQ: u32 = 0xFFFFFB;
pub const VEC_FIRQ: u32 = 0xFFFFFE;
pub const VEC_RESET: u32 = 0x000001;
pub const VRAM_PATTERN: u32 = 0xFDFE00;
pub const VRAM_BG0: u32 = 0xFFC9E0;
pub const VRAM_BG1: u32 = 0xFFD340;
pub const VRAM_BG2: u32 = 0xFFDCA0;
pub const VRAM_PALETTE: u32 = 0xFFE600;
pub const VRAM_FONT: u32 = 0xFFF600;
pub const SCREEN_W: u32 = 320;
pub const SCREEN_H: u32 = 240;
pub const V_CTRL: u32 = 0xFFFF80;
pub const V_LINE: u32 = 0xFFFF81;
pub const V_STAT: u32 = 0xFFFF82;
pub const V_IRQLINE: u32 = 0xFFFF83;
pub const V_SCROLLX0: u32 = 0xFFFF84;
pub const V_SCROLLY0: u32 = 0xFFFF86;
pub const V_SCROLLX1: u32 = 0xFFFF88;
pub const V_SCROLLY1: u32 = 0xFFFF8A;
pub const V_BANK: u32 = 0xFFFF8C;
pub const V_BRIGHT: u32 = 0xFFFF8D;
pub const V_IRQEN: u32 = 0xFFFF8E;
pub const V_PALGRP: u32 = 0xFFFF8F;
pub const V_IRQST: u32 = 0xFFFF90;
pub const SPR_OAM: u32 = 0xFDF600;
pub const SPR_COUNT: u32 = 256;
pub const SPR_SIZE: u32 = 8;
pub const SPR_X: u32 = 0;
pub const SPR_Y: u32 = 1;
pub const SPR_TILE: u32 = 2;
pub const SPR_ATTR: u32 = 3;
pub const SPR_PAL: u32 = 4;
pub const SPR_XH: u32 = 5;
pub const SPR_EXT: u32 = 6;
pub const SPR_PRIO: u32 = 0b10000000;
pub const SPR_FLIPH: u32 = 0b01000000;
pub const SPR_FLIPV: u32 = 0b00100000;
pub const SPR_EXTSZ: u32 = 0b00000001;
pub const SPR_SZ_8: u32 = 0b00000000;
pub const SPR_SZ_816: u32 = 0b00001000;
pub const SPR_SZ_16: u32 = 0b00010000;
pub const SPR_SZ_32: u32 = 0b00011000;
pub const SPR_XSZ_168: u32 = 0b00000000;
pub const SPR_XSZ_24: u32 = 0b00001000;
pub const KB_CTRL: u32 = 0xFFFFC0;
pub const KB_STAT: u32 = 0xFFFFC1;
pub const KB_SCAN: u32 = 0xFFFFC2;
pub const KB_DATA: u32 = 0xFFFFC3;
pub const KB_MOD: u32 = 0xFFFFC4;
pub const KB_ROW: u32 = 0xFFFFC5;
pub const KB_COL: u32 = 0xFFFFC6;
pub const KB_FIFO: u32 = 0xFFFFC7;
pub const KB_FIFOSTAT: u32 = 0xFFFFC8;
pub const KB_REP: u32 = 0xFFFFC9;
pub const KB_DELAY: u32 = 0xFFFFCA;
pub const KB_RATE: u32 = 0xFFFFCB;
pub const KB_LED: u32 = 0xFFFFCC;
pub const KB_CMD: u32 = 0xFFFFCD;
pub const KB_MATRIX: u32 = 0xFFFFCE;
pub const KB_PAD: u32 = 0xFFFFDC;
pub const BTN_A: u32 = 0b10000000;
pub const BTN_B: u32 = 0b01000000;
pub const BTN_START: u32 = 0b00100000;
pub const BTN_SELECT: u32 = 0b00010000;
pub const BTN_UP: u32 = 0b00001000;
pub const BTN_DOWN: u32 = 0b00000100;
pub const BTN_LEFT: u32 = 0b00000010;
pub const BTN_RIGHT: u32 = 0b00000001;
pub const AUD_BASE: u32 = 0xFFFF00;
pub const AUD_STRIDE: u32 = 8;
pub const AUD_VOICES: u32 = 8;
pub const AV_VOL_L: u32 = 0;
pub const AV_VOL_R: u32 = 1;
pub const AV_PITCH_L: u32 = 2;
pub const AV_PITCH_H: u32 = 3;
pub const AV_SRC: u32 = 4;
pub const AV_ADSR1: u32 = 5;
pub const AV_ADSR2: u32 = 6;
pub const AV_CTRL: u32 = 7;
pub const AUD_V0_VOL_L: u32 = 0xFFFF00;
pub const AUD_V0_VOL_R: u32 = 0xFFFF01;
pub const AUD_V0_PITCH_L: u32 = 0xFFFF02;
pub const AUD_V0_PITCH_H: u32 = 0xFFFF03;
pub const AUD_V0_SRC: u32 = 0xFFFF04;
pub const AUD_V0_ADSR1: u32 = 0xFFFF05;
pub const AUD_V0_ADSR2: u32 = 0xFFFF06;
pub const AUD_V0_CTRL: u32 = 0xFFFF07;
pub const AUD_V1_VOL_L: u32 = 0xFFFF08;
pub const AUD_V1_VOL_R: u32 = 0xFFFF09;
pub const AUD_V1_PITCH_L: u32 = 0xFFFF0A;
pub const AUD_V1_PITCH_H: u32 = 0xFFFF0B;
pub const AUD_V1_SRC: u32 = 0xFFFF0C;
pub const AUD_V1_ADSR1: u32 = 0xFFFF0D;
pub const AUD_V1_ADSR2: u32 = 0xFFFF0E;
pub const AUD_V1_CTRL: u32 = 0xFFFF0F;
pub const AUD_V2_VOL_L: u32 = 0xFFFF10;
pub const AUD_V2_VOL_R: u32 = 0xFFFF11;
pub const AUD_V2_PITCH_L: u32 = 0xFFFF12;
pub const AUD_V2_PITCH_H: u32 = 0xFFFF13;
pub const AUD_V2_SRC: u32 = 0xFFFF14;
pub const AUD_V2_ADSR1: u32 = 0xFFFF15;
pub const AUD_V2_ADSR2: u32 = 0xFFFF16;
pub const AUD_V2_CTRL: u32 = 0xFFFF17;
pub const AUD_V3_VOL_L: u32 = 0xFFFF18;
pub const AUD_V3_VOL_R: u32 = 0xFFFF19;
pub const AUD_V3_PITCH_L: u32 = 0xFFFF1A;
pub const AUD_V3_PITCH_H: u32 = 0xFFFF1B;
pub const AUD_V3_SRC: u32 = 0xFFFF1C;
pub const AUD_V3_ADSR1: u32 = 0xFFFF1D;
pub const AUD_V3_ADSR2: u32 = 0xFFFF1E;
pub const AUD_V3_CTRL: u32 = 0xFFFF1F;
pub const AUD_V4_VOL_L: u32 = 0xFFFF20;
pub const AUD_V4_VOL_R: u32 = 0xFFFF21;
pub const AUD_V4_PITCH_L: u32 = 0xFFFF22;
pub const AUD_V4_PITCH_H: u32 = 0xFFFF23;
pub const AUD_V4_SRC: u32 = 0xFFFF24;
pub const AUD_V4_ADSR1: u32 = 0xFFFF25;
pub const AUD_V4_ADSR2: u32 = 0xFFFF26;
pub const AUD_V4_CTRL: u32 = 0xFFFF27;
pub const AUD_V5_VOL_L: u32 = 0xFFFF28;
pub const AUD_V5_VOL_R: u32 = 0xFFFF29;
pub const AUD_V5_PITCH_L: u32 = 0xFFFF2A;
pub const AUD_V5_PITCH_H: u32 = 0xFFFF2B;
pub const AUD_V5_SRC: u32 = 0xFFFF2C;
pub const AUD_V5_ADSR1: u32 = 0xFFFF2D;
pub const AUD_V5_ADSR2: u32 = 0xFFFF2E;
pub const AUD_V5_CTRL: u32 = 0xFFFF2F;
pub const AUD_V6_VOL_L: u32 = 0xFFFF30;
pub const AUD_V6_VOL_R: u32 = 0xFFFF31;
pub const AUD_V6_PITCH_L: u32 = 0xFFFF32;
pub const AUD_V6_PITCH_H: u32 = 0xFFFF33;
pub const AUD_V6_SRC: u32 = 0xFFFF34;
pub const AUD_V6_ADSR1: u32 = 0xFFFF35;
pub const AUD_V6_ADSR2: u32 = 0xFFFF36;
pub const AUD_V6_CTRL: u32 = 0xFFFF37;
pub const AUD_V7_VOL_L: u32 = 0xFFFF38;
pub const AUD_V7_VOL_R: u32 = 0xFFFF39;
pub const AUD_V7_PITCH_L: u32 = 0xFFFF3A;
pub const AUD_V7_PITCH_H: u32 = 0xFFFF3B;
pub const AUD_V7_SRC: u32 = 0xFFFF3C;
pub const AUD_V7_ADSR1: u32 = 0xFFFF3D;
pub const AUD_V7_ADSR2: u32 = 0xFFFF3E;
pub const AUD_V7_CTRL: u32 = 0xFFFF3F;
pub const AUD_MVOL_L: u32 = 0xFFFF40;
pub const AUD_MVOL_R: u32 = 0xFFFF41;
pub const AUD_KON: u32 = 0xFFFF42;
pub const AUD_KOFF: u32 = 0xFFFF43;
pub const AUD_STAT: u32 = 0xFFFF44;
pub const AUD_ENDX: u32 = 0xFFFF45;
pub const AUD_IRQEN: u32 = 0xFFFF46;
pub const AUD_DIR: u32 = 0xFFFF47;
pub const AUD_NOISE: u32 = 0xFFFF48;
pub const AUD_ACTRL: u32 = 0xFFFF49;
pub const AUD_ENVX: u32 = 0xFFFF50;
pub const AUD_PITCH_1_0: u32 = 0x1000;
pub const BG0_PRIORITY: u32 = 0b00000001;
pub const BG0_PALETTE: u32 = 0b01111000;
pub const BG1_PALETTE: u32 = 0b00111100;
pub const BOOT_ROM: u32 = 0xFBB600;
pub const FIXED_ENTRY: u32 = 0x000100;
pub const CART_WIN: u32 = 0xFBF600;
pub const CART_WIN_END: u32 = 0xFCF5FF;
pub const CART_STAT: u32 = 0xFFFEF0;
pub const CART_BANK: u32 = 0xFFFEF1;
pub const CART_CTRL: u32 = 0xFFFEF2;
pub const CART_SBANK: u32 = 0xFFFEF3;
pub const CART_BANK_SIZE: u32 = 65536;
pub const CH_LEN: u32 = 64;
pub const CH_MAGIC: u32 = 0;
pub const CH_VER_MAJOR: u32 = 4;
pub const CH_VER_MINOR: u32 = 5;
pub const CH_FLAGS: u32 = 6;
pub const CH_MAPPER: u32 = 8;
pub const CH_HDR_SIZE: u32 = 9;
pub const CH_SAVE_KB: u32 = 10;
pub const CH_IMAGE_LEN: u32 = 12;
pub const CH_ENTRY: u32 = 15;
pub const CH_CHECKSUM: u32 = 18;
pub const CH_VEC_SWI: u32 = 22;
pub const CH_VEC_NMI: u32 = 25;
pub const CH_VEC_IRQ: u32 = 28;
pub const CH_VEC_FIRQ: u32 = 31;
pub const CH_TITLE: u32 = 33;
pub const CH_RESERVED: u32 = 49;
pub const MAPPER_ROM: u32 = 0;
pub const MAPPER_ROM_SAVE: u32 = 1;
pub const ERR_MAGIC: u32 = 1;
pub const ERR_MAJOR: u32 = 2;
pub const ERR_MINOR: u32 = 3;
pub const ERR_FLAGS: u32 = 4;
pub const ERR_MAPPER: u32 = 5;
pub const ERR_HEADER: u32 = 6;
pub const ERR_ENTRY: u32 = 7;
pub const ERR_CHECKSUM: u32 = 8;
pub const ERR_RAM: u32 = 9;
pub const ERR_RETURNED: u32 = 10;
pub const ERR_INTERRUPT: u32 = 11;
pub const ERR_LENGTH: u32 = 12;
pub const ERR_NOCART: u32 = 13;
pub const ERR_SYSTEM: u32 = 14;
pub const ERR_SEAL: u32 = 15;
pub const ENGINE_KEY: u32 = 0xFFFE00;
pub const ENGINE_SEAL: u32 = 0xFFFE20;
pub const ENGINE_DST: u32 = 0xFFFE40;
pub const ENGINE_CTRL: u32 = 0xFFFE43;
pub const NVRAM_BASE: u32 = 0xFFFE80;
pub const SYS_LOCK: u32 = 0xFFFED0;
pub const SYS_RESET: u32 = 0xFFFED1;
pub const ENGINE_CHECK: u32 = 0x01;
pub const ENGINE_LOAD: u32 = 0x02;
pub const ENGINE_OK: u32 = 0x01;
pub const ENGINE_SEALED: u32 = 0x02;
pub const ENGINE_FAIL: u32 = 0x80;
pub const CF_ENCRYPTED: u32 = 0x01;
pub const CF_SEALED: u32 = 0x02;

pub struct OpInfo { pub opcode: u8, pub group: &'static str, pub mnemonic: &'static str, pub operands: &'static str, pub cycles: &'static str, pub legal: bool }

/// Default operands; the postbyte picks the real registers. `cycles` is text like "1", "1+n" or "2-3".
pub const OPCODES: [OpInfo; 256] = [
    OpInfo { opcode: 0x00, group: "Data Transfer", mnemonic: "NOP", operands: "—", cycles: "1", legal: true },
    OpInfo { opcode: 0x01, group: "Data Transfer", mnemonic: "LD", operands: "A,#imm", cycles: "2", legal: true },
    OpInfo { opcode: 0x02, group: "Data Transfer", mnemonic: "LD", operands: "B,#imm", cycles: "2", legal: true },
    OpInfo { opcode: 0x03, group: "Data Transfer", mnemonic: "LD", operands: "D,#imm", cycles: "2", legal: true },
    OpInfo { opcode: 0x04, group: "Data Transfer", mnemonic: "LD", operands: "E,#imm", cycles: "2", legal: true },
    OpInfo { opcode: 0x05, group: "Data Transfer", mnemonic: "LD", operands: "F,#imm", cycles: "2", legal: true },
    OpInfo { opcode: 0x06, group: "Data Transfer", mnemonic: "LD", operands: "W,#imm", cycles: "2", legal: true },
    OpInfo { opcode: 0x07, group: "Data Transfer", mnemonic: "LD", operands: "Q,#imm", cycles: "3", legal: true },
    OpInfo { opcode: 0x08, group: "Data Transfer", mnemonic: "MOV", operands: "reg,reg", cycles: "1", legal: true },
    OpInfo { opcode: 0x09, group: "Data Transfer", mnemonic: "ST", operands: "A,<dp", cycles: "3", legal: true },
    OpInfo { opcode: 0x0A, group: "Data Transfer", mnemonic: "ST", operands: "B,<dp", cycles: "3", legal: true },
    OpInfo { opcode: 0x0B, group: "Data Transfer", mnemonic: "ST", operands: "D,<dp", cycles: "3", legal: true },
    OpInfo { opcode: 0x0C, group: "Data Transfer", mnemonic: "ST", operands: "E,<dp", cycles: "3", legal: true },
    OpInfo { opcode: 0x0D, group: "Data Transfer", mnemonic: "ST", operands: "F,<dp", cycles: "3", legal: true },
    OpInfo { opcode: 0x0E, group: "Data Transfer", mnemonic: "ST", operands: "W,<dp", cycles: "3", legal: true },
    OpInfo { opcode: 0x0F, group: "Data Transfer", mnemonic: "ST", operands: "Q,<dp", cycles: "4", legal: true },
    OpInfo { opcode: 0x10, group: "Arithmetic", mnemonic: "ADD", operands: "A,B", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x11, group: "Arithmetic", mnemonic: "ADC", operands: "A,B", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x12, group: "Arithmetic", mnemonic: "SUB", operands: "A,B", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x13, group: "Arithmetic", mnemonic: "SBC", operands: "A,B", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x14, group: "Arithmetic", mnemonic: "AND", operands: "A,B", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x15, group: "Arithmetic", mnemonic: "OR", operands: "A,B", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x16, group: "Arithmetic", mnemonic: "XOR", operands: "A,B", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x17, group: "Arithmetic", mnemonic: "NOT", operands: "A", cycles: "1", legal: true },
    OpInfo { opcode: 0x18, group: "Arithmetic", mnemonic: "INC", operands: "A,#count", cycles: "1+n", legal: true },
    OpInfo { opcode: 0x19, group: "Arithmetic", mnemonic: "DEC", operands: "A,#count", cycles: "1+n", legal: true },
    OpInfo { opcode: 0x1A, group: "Arithmetic", mnemonic: "NEG", operands: "A", cycles: "1", legal: true },
    OpInfo { opcode: 0x1B, group: "Arithmetic", mnemonic: "CMP", operands: "A,B", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x1C, group: "Arithmetic", mnemonic: "MUL", operands: "A,B", cycles: "2", legal: true },
    OpInfo { opcode: 0x1D, group: "Arithmetic", mnemonic: "DIV", operands: "D,E", cycles: "5", legal: true },
    OpInfo { opcode: 0x1E, group: "Arithmetic", mnemonic: "MOD", operands: "D,E", cycles: "5", legal: true },
    OpInfo { opcode: 0x1F, group: "Arithmetic", mnemonic: "CLR", operands: "A", cycles: "1", legal: true },
    OpInfo { opcode: 0x20, group: "Shifts, Rotates, Extensions", mnemonic: "ASL", operands: "reg,#count", cycles: "1+n", legal: true },
    OpInfo { opcode: 0x21, group: "Shifts, Rotates, Extensions", mnemonic: "ASR", operands: "reg,#count", cycles: "1+n", legal: true },
    OpInfo { opcode: 0x22, group: "Shifts, Rotates, Extensions", mnemonic: "LSL", operands: "reg,#count", cycles: "1+n", legal: true },
    OpInfo { opcode: 0x23, group: "Shifts, Rotates, Extensions", mnemonic: "LSR", operands: "reg,#count", cycles: "1+n", legal: true },
    OpInfo { opcode: 0x24, group: "Shifts, Rotates, Extensions", mnemonic: "ROL", operands: "reg,#count", cycles: "1+n", legal: true },
    OpInfo { opcode: 0x25, group: "Shifts, Rotates, Extensions", mnemonic: "ROR", operands: "reg,#count", cycles: "1+n", legal: true },
    OpInfo { opcode: 0x26, group: "Shifts, Rotates, Extensions", mnemonic: "RCL", operands: "reg,#count", cycles: "1+n", legal: true },
    OpInfo { opcode: 0x27, group: "Shifts, Rotates, Extensions", mnemonic: "RCR", operands: "reg,#count", cycles: "1+n", legal: true },
    OpInfo { opcode: 0x28, group: "Shifts, Rotates, Extensions", mnemonic: "TST", operands: "reg", cycles: "1", legal: true },
    OpInfo { opcode: 0x29, group: "Shifts, Rotates, Extensions", mnemonic: "BIT", operands: "dest,src", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x2A, group: "Shifts, Rotates, Extensions", mnemonic: "COM", operands: "reg", cycles: "1", legal: true },
    OpInfo { opcode: 0x2B, group: "Shifts, Rotates, Extensions", mnemonic: "SWAP", operands: "reg", cycles: "1", legal: true },
    OpInfo { opcode: 0x2C, group: "Shifts, Rotates, Extensions", mnemonic: "EXT", operands: "A,D", cycles: "1", legal: true },
    OpInfo { opcode: 0x2D, group: "Shifts, Rotates, Extensions", mnemonic: "ZEXT", operands: "A,D", cycles: "1", legal: true },
    OpInfo { opcode: 0x2E, group: "Shifts, Rotates, Extensions", mnemonic: "MAQ", operands: "D,W", cycles: "2", legal: true },
    OpInfo { opcode: 0x2F, group: "Shifts, Rotates, Extensions", mnemonic: "ADDZ", operands: "addr,#imm", cycles: "5", legal: true },
    OpInfo { opcode: 0x30, group: "Short Branches", mnemonic: "BRA", operands: "rel8", cycles: "2", legal: true },
    OpInfo { opcode: 0x31, group: "Short Branches", mnemonic: "BRN", operands: "rel8", cycles: "1", legal: true },
    OpInfo { opcode: 0x32, group: "Short Branches", mnemonic: "BEQ", operands: "rel8", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x33, group: "Short Branches", mnemonic: "BNE", operands: "rel8", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x34, group: "Short Branches", mnemonic: "BCC", operands: "rel8", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x35, group: "Short Branches", mnemonic: "BCS", operands: "rel8", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x36, group: "Short Branches", mnemonic: "BHI", operands: "rel8", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x37, group: "Short Branches", mnemonic: "BLS", operands: "rel8", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x38, group: "Short Branches", mnemonic: "BGE", operands: "rel8", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x39, group: "Short Branches", mnemonic: "BLT", operands: "rel8", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x3A, group: "Short Branches", mnemonic: "BGT", operands: "rel8", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x3B, group: "Short Branches", mnemonic: "BLE", operands: "rel8", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x3C, group: "Short Branches", mnemonic: "BMI", operands: "rel8", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x3D, group: "Short Branches", mnemonic: "BPL", operands: "rel8", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x3E, group: "Short Branches", mnemonic: "BVS", operands: "rel8", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x3F, group: "Short Branches", mnemonic: "BVC", operands: "rel8", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x40, group: "Jumps, Branches, LD Direct Page", mnemonic: "JMP", operands: "addr24", cycles: "2", legal: true },
    OpInfo { opcode: 0x41, group: "Jumps, Branches, LD Direct Page", mnemonic: "JSR", operands: "addr24", cycles: "3", legal: true },
    OpInfo { opcode: 0x42, group: "Jumps, Branches, LD Direct Page", mnemonic: "RTS", operands: "—", cycles: "3", legal: true },
    OpInfo { opcode: 0x43, group: "Jumps, Branches, LD Direct Page", mnemonic: "RTI", operands: "—", cycles: "3+E", legal: true },
    OpInfo { opcode: 0x44, group: "Jumps, Branches, LD Direct Page", mnemonic: "BSR", operands: "rel8", cycles: "2", legal: true },
    OpInfo { opcode: 0x45, group: "Jumps, Branches, LD Direct Page", mnemonic: "LBRA", operands: "rel16", cycles: "2", legal: true },
    OpInfo { opcode: 0x46, group: "Jumps, Branches, LD Direct Page", mnemonic: "LD", operands: "A,<dp", cycles: "3", legal: true },
    OpInfo { opcode: 0x47, group: "Jumps, Branches, LD Direct Page", mnemonic: "LD", operands: "B,<dp", cycles: "3", legal: true },
    OpInfo { opcode: 0x48, group: "Jumps, Branches, LD Direct Page", mnemonic: "LD", operands: "D,<dp", cycles: "3", legal: true },
    OpInfo { opcode: 0x49, group: "Jumps, Branches, LD Direct Page", mnemonic: "LD", operands: "E,<dp", cycles: "3", legal: true },
    OpInfo { opcode: 0x4A, group: "Jumps, Branches, LD Direct Page", mnemonic: "LD", operands: "F,<dp", cycles: "3", legal: true },
    OpInfo { opcode: 0x4B, group: "Jumps, Branches, LD Direct Page", mnemonic: "LD", operands: "W,<dp", cycles: "3", legal: true },
    OpInfo { opcode: 0x4C, group: "Jumps, Branches, LD Direct Page", mnemonic: "LD", operands: "Q,<dp", cycles: "4", legal: true },
    OpInfo { opcode: 0x4D, group: "Jumps, Branches, LD Direct Page", mnemonic: "LBSR", operands: "rel16", cycles: "3", legal: true },
    OpInfo { opcode: 0x4E, group: "Jumps, Branches, LD Direct Page", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x4F, group: "Jumps, Branches, LD Direct Page", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x50, group: "Specialized Operations", mnemonic: "PACK", operands: "D,A,B", cycles: "1", legal: true },
    OpInfo { opcode: 0x51, group: "Specialized Operations", mnemonic: "UNP", operands: "A,B,D", cycles: "1", legal: true },
    OpInfo { opcode: 0x52, group: "Specialized Operations", mnemonic: "BEXT", operands: "A,D,#pos,#len", cycles: "2", legal: true },
    OpInfo { opcode: 0x53, group: "Specialized Operations", mnemonic: "BINS", operands: "D,A,#pos,#len", cycles: "2", legal: true },
    OpInfo { opcode: 0x54, group: "Specialized Operations", mnemonic: "LEA", operands: "X,addr", cycles: "1", legal: true },
    OpInfo { opcode: 0x55, group: "Specialized Operations", mnemonic: "SMODX", operands: "X,reg or #imm", cycles: "1", legal: true },
    OpInfo { opcode: 0x56, group: "Specialized Operations", mnemonic: "PUSH", operands: "reg", cycles: "2-3", legal: true },
    OpInfo { opcode: 0x57, group: "Specialized Operations", mnemonic: "PUSH2", operands: "reg,reg", cycles: "3-4", legal: true },
    OpInfo { opcode: 0x58, group: "Specialized Operations", mnemonic: "POP", operands: "reg", cycles: "2-3", legal: true },
    OpInfo { opcode: 0x59, group: "Specialized Operations", mnemonic: "POP2", operands: "reg,reg", cycles: "3-4", legal: true },
    OpInfo { opcode: 0x5A, group: "Specialized Operations", mnemonic: "LDI", operands: "reg,#imm", cycles: "1-2", legal: true },
    OpInfo { opcode: 0x5B, group: "Specialized Operations", mnemonic: "MOVQ", operands: "Q,src", cycles: "3", legal: true },
    OpInfo { opcode: 0x5C, group: "Specialized Operations", mnemonic: "MULD", operands: "D,W", cycles: "2", legal: true },
    OpInfo { opcode: 0x5D, group: "Specialized Operations", mnemonic: "DAA", operands: "—", cycles: "1", legal: true },
    OpInfo { opcode: 0x5E, group: "Specialized Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x5F, group: "Specialized Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x60, group: "Block Memory Operations", mnemonic: "MOVB", operands: "[X],[Y],#count", cycles: "1/byte", legal: true },
    OpInfo { opcode: 0x61, group: "Block Memory Operations", mnemonic: "MOVW", operands: "[X],[Y],#count", cycles: "1/2-words", legal: true },
    OpInfo { opcode: 0x62, group: "Block Memory Operations", mnemonic: "FILL", operands: "[X],#val,#count", cycles: "1/2-words", legal: true },
    OpInfo { opcode: 0x63, group: "Block Memory Operations", mnemonic: "CPY", operands: "[X],[Y],#count", cycles: "1/2-words", legal: true },
    OpInfo { opcode: 0x64, group: "Block Memory Operations", mnemonic: "SCNB", operands: "[X],#val,#count", cycles: "1/byte", legal: true },
    OpInfo { opcode: 0x65, group: "Block Memory Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x66, group: "Block Memory Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x67, group: "Block Memory Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x68, group: "Block Memory Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x69, group: "Block Memory Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x6A, group: "Block Memory Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x6B, group: "Block Memory Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x6C, group: "Block Memory Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x6D, group: "Block Memory Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x6E, group: "Block Memory Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x6F, group: "Block Memory Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x70, group: "Flag Operations", mnemonic: "CLC", operands: "—", cycles: "1", legal: true },
    OpInfo { opcode: 0x71, group: "Flag Operations", mnemonic: "SEC", operands: "—", cycles: "1", legal: true },
    OpInfo { opcode: 0x72, group: "Flag Operations", mnemonic: "CLV", operands: "—", cycles: "1", legal: true },
    OpInfo { opcode: 0x73, group: "Flag Operations", mnemonic: "SEV", operands: "—", cycles: "1", legal: true },
    OpInfo { opcode: 0x74, group: "Flag Operations", mnemonic: "CLI", operands: "—", cycles: "1", legal: true },
    OpInfo { opcode: 0x75, group: "Flag Operations", mnemonic: "SEI", operands: "—", cycles: "1", legal: true },
    OpInfo { opcode: 0x76, group: "Flag Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x77, group: "Flag Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x78, group: "Flag Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x79, group: "Flag Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x7A, group: "Flag Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x7B, group: "Flag Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x7C, group: "Flag Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x7D, group: "Flag Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x7E, group: "Flag Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x7F, group: "Flag Operations", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x80, group: "Long Conditional Branches", mnemonic: "LBEQ", operands: "rel16", cycles: "2-3", legal: true },
    OpInfo { opcode: 0x81, group: "Long Conditional Branches", mnemonic: "LBNE", operands: "rel16", cycles: "2-3", legal: true },
    OpInfo { opcode: 0x82, group: "Long Conditional Branches", mnemonic: "LBCC", operands: "rel16", cycles: "2-3", legal: true },
    OpInfo { opcode: 0x83, group: "Long Conditional Branches", mnemonic: "LBCS", operands: "rel16", cycles: "2-3", legal: true },
    OpInfo { opcode: 0x84, group: "Long Conditional Branches", mnemonic: "LBHI", operands: "rel16", cycles: "2-3", legal: true },
    OpInfo { opcode: 0x85, group: "Long Conditional Branches", mnemonic: "LBLS", operands: "rel16", cycles: "2-3", legal: true },
    OpInfo { opcode: 0x86, group: "Long Conditional Branches", mnemonic: "LBGE", operands: "rel16", cycles: "2-3", legal: true },
    OpInfo { opcode: 0x87, group: "Long Conditional Branches", mnemonic: "LBLT", operands: "rel16", cycles: "2-3", legal: true },
    OpInfo { opcode: 0x88, group: "Long Conditional Branches", mnemonic: "LBGT", operands: "rel16", cycles: "2-3", legal: true },
    OpInfo { opcode: 0x89, group: "Long Conditional Branches", mnemonic: "LBLE", operands: "rel16", cycles: "2-3", legal: true },
    OpInfo { opcode: 0x8A, group: "Long Conditional Branches", mnemonic: "LBMI", operands: "rel16", cycles: "2-3", legal: true },
    OpInfo { opcode: 0x8B, group: "Long Conditional Branches", mnemonic: "LBPL", operands: "rel16", cycles: "2-3", legal: true },
    OpInfo { opcode: 0x8C, group: "Long Conditional Branches", mnemonic: "LBVS", operands: "rel16", cycles: "2-3", legal: true },
    OpInfo { opcode: 0x8D, group: "Long Conditional Branches", mnemonic: "LBVC", operands: "rel16", cycles: "2-3", legal: true },
    OpInfo { opcode: 0x8E, group: "Long Conditional Branches", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x8F, group: "Long Conditional Branches", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x90, group: "Bit Operations and Combined Control", mnemonic: "BSET", operands: "addr,#bit", cycles: "4", legal: true },
    OpInfo { opcode: 0x91, group: "Bit Operations and Combined Control", mnemonic: "BCLR", operands: "addr,#bit", cycles: "4", legal: true },
    OpInfo { opcode: 0x92, group: "Bit Operations and Combined Control", mnemonic: "BCHG", operands: "addr,#bit", cycles: "4", legal: true },
    OpInfo { opcode: 0x93, group: "Bit Operations and Combined Control", mnemonic: "BTST", operands: "addr,#bit", cycles: "3", legal: true },
    OpInfo { opcode: 0x94, group: "Bit Operations and Combined Control", mnemonic: "JBS", operands: "addr,#bit,rel", cycles: "4", legal: true },
    OpInfo { opcode: 0x95, group: "Bit Operations and Combined Control", mnemonic: "JBC", operands: "addr,#bit,rel", cycles: "4", legal: true },
    OpInfo { opcode: 0x96, group: "Bit Operations and Combined Control", mnemonic: "DBNZ", operands: "reg,rel", cycles: "2", legal: true },
    OpInfo { opcode: 0x97, group: "Bit Operations and Combined Control", mnemonic: "TBZ", operands: "reg,#bit,rel", cycles: "2", legal: true },
    OpInfo { opcode: 0x98, group: "Bit Operations and Combined Control", mnemonic: "TBNZ", operands: "reg,#bit,rel", cycles: "2", legal: true },
    OpInfo { opcode: 0x99, group: "Bit Operations and Combined Control", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x9A, group: "Bit Operations and Combined Control", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x9B, group: "Bit Operations and Combined Control", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x9C, group: "Bit Operations and Combined Control", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x9D, group: "Bit Operations and Combined Control", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x9E, group: "Bit Operations and Combined Control", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0x9F, group: "Bit Operations and Combined Control", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xA0, group: "Register Exchange and Reserved", mnemonic: "EXG", operands: "reg,reg", cycles: "1", legal: true },
    OpInfo { opcode: 0xA1, group: "Register Exchange and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xA2, group: "Register Exchange and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xA3, group: "Register Exchange and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xA4, group: "Register Exchange and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xA5, group: "Register Exchange and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xA6, group: "Register Exchange and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xA7, group: "Register Exchange and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xA8, group: "Register Exchange and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xA9, group: "Register Exchange and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xAA, group: "Register Exchange and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xAB, group: "Register Exchange and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xAC, group: "Register Exchange and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xAD, group: "Register Exchange and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xAE, group: "Register Exchange and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xAF, group: "Register Exchange and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xB0, group: "Wait Operations and Reserved", mnemonic: "WAIT", operands: "—", cycles: "2+", legal: true },
    OpInfo { opcode: 0xB1, group: "Wait Operations and Reserved", mnemonic: "WAITV", operands: "—", cycles: "2+", legal: true },
    OpInfo { opcode: 0xB2, group: "Wait Operations and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xB3, group: "Wait Operations and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xB4, group: "Wait Operations and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xB5, group: "Wait Operations and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xB6, group: "Wait Operations and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xB7, group: "Wait Operations and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xB8, group: "Wait Operations and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xB9, group: "Wait Operations and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xBA, group: "Wait Operations and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xBB, group: "Wait Operations and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xBC, group: "Wait Operations and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xBD, group: "Wait Operations and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xBE, group: "Wait Operations and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xBF, group: "Wait Operations and Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xC0, group: "LD Extended (Absolute 24-bit addressing)", mnemonic: "LD", operands: "A,$addr", cycles: "3", legal: true },
    OpInfo { opcode: 0xC1, group: "LD Extended (Absolute 24-bit addressing)", mnemonic: "LD", operands: "B,$addr", cycles: "3", legal: true },
    OpInfo { opcode: 0xC2, group: "LD Extended (Absolute 24-bit addressing)", mnemonic: "LD", operands: "D,$addr", cycles: "3", legal: true },
    OpInfo { opcode: 0xC3, group: "LD Extended (Absolute 24-bit addressing)", mnemonic: "LD", operands: "E,$addr", cycles: "3", legal: true },
    OpInfo { opcode: 0xC4, group: "LD Extended (Absolute 24-bit addressing)", mnemonic: "LD", operands: "F,$addr", cycles: "3", legal: true },
    OpInfo { opcode: 0xC5, group: "LD Extended (Absolute 24-bit addressing)", mnemonic: "LD", operands: "W,$addr", cycles: "3", legal: true },
    OpInfo { opcode: 0xC6, group: "LD Extended (Absolute 24-bit addressing)", mnemonic: "LD", operands: "Q,$addr", cycles: "4", legal: true },
    OpInfo { opcode: 0xC7, group: "LD Extended (Absolute 24-bit addressing)", mnemonic: "ST", operands: "A,$addr", cycles: "3", legal: true },
    OpInfo { opcode: 0xC8, group: "LD Extended (Absolute 24-bit addressing)", mnemonic: "ST", operands: "B,$addr", cycles: "3", legal: true },
    OpInfo { opcode: 0xC9, group: "LD Extended (Absolute 24-bit addressing)", mnemonic: "ST", operands: "D,$addr", cycles: "3", legal: true },
    OpInfo { opcode: 0xCA, group: "LD Extended (Absolute 24-bit addressing)", mnemonic: "ST", operands: "E,$addr", cycles: "3", legal: true },
    OpInfo { opcode: 0xCB, group: "LD Extended (Absolute 24-bit addressing)", mnemonic: "ST", operands: "F,$addr", cycles: "3", legal: true },
    OpInfo { opcode: 0xCC, group: "LD Extended (Absolute 24-bit addressing)", mnemonic: "ST", operands: "W,$addr", cycles: "3", legal: true },
    OpInfo { opcode: 0xCD, group: "LD Extended (Absolute 24-bit addressing)", mnemonic: "ST", operands: "Q,$addr", cycles: "4", legal: true },
    OpInfo { opcode: 0xCE, group: "LD Extended (Absolute 24-bit addressing)", mnemonic: "LD", operands: "X,$addr", cycles: "4", legal: true },
    OpInfo { opcode: 0xCF, group: "LD Extended (Absolute 24-bit addressing)", mnemonic: "LD", operands: "Y,$addr", cycles: "4", legal: true },
    OpInfo { opcode: 0xD0, group: "LD/ST Indexed", mnemonic: "LD", operands: "A,indexed", cycles: "3", legal: true },
    OpInfo { opcode: 0xD1, group: "LD/ST Indexed", mnemonic: "LD", operands: "B,indexed", cycles: "3", legal: true },
    OpInfo { opcode: 0xD2, group: "LD/ST Indexed", mnemonic: "LD", operands: "D,indexed", cycles: "3", legal: true },
    OpInfo { opcode: 0xD3, group: "LD/ST Indexed", mnemonic: "LD", operands: "E,indexed", cycles: "3", legal: true },
    OpInfo { opcode: 0xD4, group: "LD/ST Indexed", mnemonic: "LD", operands: "F,indexed", cycles: "3", legal: true },
    OpInfo { opcode: 0xD5, group: "LD/ST Indexed", mnemonic: "LD", operands: "W,indexed", cycles: "3", legal: true },
    OpInfo { opcode: 0xD6, group: "LD/ST Indexed", mnemonic: "LD", operands: "Q,indexed", cycles: "4", legal: true },
    OpInfo { opcode: 0xD7, group: "LD/ST Indexed", mnemonic: "ST", operands: "A,indexed", cycles: "3", legal: true },
    OpInfo { opcode: 0xD8, group: "LD/ST Indexed", mnemonic: "ST", operands: "B,indexed", cycles: "3", legal: true },
    OpInfo { opcode: 0xD9, group: "LD/ST Indexed", mnemonic: "ST", operands: "D,indexed", cycles: "3", legal: true },
    OpInfo { opcode: 0xDA, group: "LD/ST Indexed", mnemonic: "ST", operands: "E,indexed", cycles: "3", legal: true },
    OpInfo { opcode: 0xDB, group: "LD/ST Indexed", mnemonic: "ST", operands: "F,indexed", cycles: "3", legal: true },
    OpInfo { opcode: 0xDC, group: "LD/ST Indexed", mnemonic: "ST", operands: "W,indexed", cycles: "3", legal: true },
    OpInfo { opcode: 0xDD, group: "LD/ST Indexed", mnemonic: "ST", operands: "Q,indexed", cycles: "4", legal: true },
    OpInfo { opcode: 0xDE, group: "LD/ST Indexed", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xDF, group: "LD/ST Indexed", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xE0, group: "Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xE1, group: "Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xE2, group: "Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xE3, group: "Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xE4, group: "Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xE5, group: "Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xE6, group: "Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xE7, group: "Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xE8, group: "Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xE9, group: "Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xEA, group: "Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xEB, group: "Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xEC, group: "Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xED, group: "Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xEE, group: "Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xEF, group: "Reserved", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xF0, group: "System Control and Prefix", mnemonic: "NMI", operands: "—", cycles: "5", legal: true },
    OpInfo { opcode: 0xF1, group: "System Control and Prefix", mnemonic: "IRQ", operands: "—", cycles: "5", legal: true },
    OpInfo { opcode: 0xF2, group: "System Control and Prefix", mnemonic: "FIRQ", operands: "—", cycles: "3", legal: true },
    OpInfo { opcode: 0xF3, group: "System Control and Prefix", mnemonic: "SWI", operands: "—", cycles: "5", legal: true },
    OpInfo { opcode: 0xF4, group: "System Control and Prefix", mnemonic: "RESET", operands: "—", cycles: "2", legal: true },
    OpInfo { opcode: 0xF5, group: "System Control and Prefix", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xF6, group: "System Control and Prefix", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xF7, group: "System Control and Prefix", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xF8, group: "System Control and Prefix", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xF9, group: "System Control and Prefix", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xFA, group: "System Control and Prefix", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xFB, group: "System Control and Prefix", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xFC, group: "System Control and Prefix", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xFD, group: "System Control and Prefix", mnemonic: "—", operands: "—", cycles: "1", legal: false },
    OpInfo { opcode: 0xFE, group: "System Control and Prefix", mnemonic: "PREFIX", operands: "—", cycles: "0", legal: true },
    OpInfo { opcode: 0xFF, group: "System Control and Prefix", mnemonic: "PREFIX", operands: "—", cycles: "0", legal: true },
];
