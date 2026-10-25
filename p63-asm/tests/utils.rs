//! Assembles the hand-written utils and runs their routines in the emulator.

use p63_asm::{assemble, Program};
use p63_core::{Addr, Bus, Flags, Ram, ReferenceInterpreter};

const UTILS: &str = "../utils";

/// Skips the utils tests in a checkout without the utils folder.
fn have_utils() -> bool {
    std::path::Path::new(UTILS).is_dir()
}

/// A routine from a utils file, wrapped in `JSR routine` / spin loop.
struct Harness {
    cpu: ReferenceInterpreter<Ram>,
    program: Program,
}

impl Harness {
    fn new(file: &str, routine: &str) -> Self {
        Self::calling(file, &[routine])
    }

    /// Calls each routine in turn, sharing memory and registers.
    fn calling(file: &str, routines: &[&str]) -> Self {
        let calls: String = routines.iter().map(|r| format!("    JSR {r}\n")).collect();
        let src = format!(
            "    ORG $1000\nSTART:\n{calls}DONE: BRA DONE\n    INCLUDE \"{UTILS}/{file}\"\n"
        );
        let program = assemble("harness", &src).unwrap_or_else(|e| panic!("{}", e[0]));
        let mut cpu = ReferenceInterpreter::new(Ram::new(0x0100_0000));
        program.load(&mut cpu.bus);
        cpu.registers.set_sp(0x8000);
        Self { cpu, program }
    }

    /// Runs from START until the routine returns to DONE.
    fn call(&mut self) -> &mut ReferenceInterpreter<Ram> {
        let start = self.program.symbol("START").unwrap();
        let done = self.program.symbol("DONE").unwrap();
        self.cpu.registers.set_pc(start);
        for _ in 0..1_000_000 {
            if self.cpu.registers.pc() == done {
                return &mut self.cpu;
            }
            self.cpu.step().expect("step");
        }
        panic!("routine never returned");
    }
}

fn call_a(file: &str, routine: &str, a: u8) -> (u8, Flags) {
    let mut h = Harness::new(file, routine);
    h.cpu.registers.set_a(a);
    let cpu = h.call();
    (cpu.registers.a(), cpu.registers.cc())
}

#[test]
fn bitops_documented_examples() {
    if !have_utils() {
        return;
    }
    let cases = [
        ("LEAD_Z", 0x08, 4),
        ("LEAD_Z", 0xFF, 0),
        ("TRAIL_Z", 0x08, 3),
        ("TRAIL_Z", 0x01, 0),
        ("LEAD_1", 0xF8, 5),
        ("LEAD_1", 0x7F, 0),
        ("TRAIL_1", 0x0F, 4),
        ("TRAIL_1", 0xFE, 0),
        ("POP_COUNT", 0xA5, 4),
        ("POP_COUNT", 0xFF, 8),
        ("PARITY", 0xA5, 0),
        ("PARITY", 0xA7, 1),
        ("REVERSE", 0x40, 0x02),
        ("REVERSE", 0xC3, 0xC3),
    ];
    let mut failures = Vec::new();
    for (routine, input, want) in cases {
        let (got, _) = call_a("bitops.p63", routine, input);
        if got != want {
            failures.push(format!("{routine}(${input:02X}) = ${got:02X}, comment says ${want:02X}"));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn bitops_all_inputs_match_rust() {
    if !have_utils() {
        return;
    }
    let mut failures = Vec::new();
    for v in 0..=255u8 {
        let checks: [(&str, u32); 7] = [
            ("LEAD_Z", v.leading_zeros()),
            ("TRAIL_Z", v.trailing_zeros()),
            ("LEAD_1", v.leading_ones()),
            ("TRAIL_1", v.trailing_ones()),
            ("POP_COUNT", v.count_ones()),
            ("PARITY", v.count_ones() & 1),
            ("REVERSE", u32::from(v.reverse_bits())),
        ];
        for (routine, want) in checks {
            let (got, _) = call_a("bitops.p63", routine, v);
            if u32::from(got) != want && failures.len() < 20 {
                failures.push(format!("{routine}(${v:02X}) = ${got:02X}, expected ${want:02X}"));
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn lead_and_trail_zero_set_z_only_for_zero_input() {
    if !have_utils() {
        return;
    }
    for routine in ["LEAD_Z", "TRAIL_Z"] {
        for v in 0..=255u8 {
            let (_, cc) = call_a("bitops.p63", routine, v);
            assert_eq!(cc.contains(Flags::Z), v == 0, "{routine}(${v:02X}) Z flag");
        }
    }
}

#[test]
fn parity_sets_z_for_even() {
    if !have_utils() {
        return;
    }
    for v in [0xA5u8, 0xA7, 0x00, 0x01] {
        let (a, cc) = call_a("bitops.p63", "PARITY", v);
        assert_eq!(cc.contains(Flags::Z), a == 0, "PARITY(${v:02X})");
    }
}

#[test]
fn sqrt_matches_rust() {
    if !have_utils() {
        return;
    }
    let mut inputs: Vec<u32> = vec![0, 1, 2, 3, 4, 15, 16, 17, 54289, 1_807_100_100, 65535, 65536, u32::MAX];
    inputs.extend((0..40u32).map(|i| i.wrapping_mul(0x9E37_79B9) ^ (i << 7)));
    let mut failures = Vec::new();
    for q in inputs {
        let mut h = Harness::new("math.p63", "SQRT");
        h.cpu.registers.set_q(q);
        let got = h.call().registers.d();
        let want = q.isqrt() as u16;
        if got != want {
            failures.push(format!("SQRT({q}) = {got}, expected {want}"));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn seed_routines_return_the_first_value_from_the_seed() {
    if !have_utils() {
        return;
    }
    let x8 = |mut s: u8| {
        s ^= s << 1;
        s ^= s >> 7;
        s ^ (s << 3)
    };
    let x16 = |mut s: u16| {
        s ^= s << 1;
        s ^= s >> 5;
        s ^ (s << 2)
    };
    let x32 = |mut s: u32| {
        s ^= s << 13;
        s ^= s >> 17;
        s ^ (s << 5)
    };

    for (seed, effective) in [(0x52u8, 0x52u8), (0x00, 0xFB)] {
        let mut h = Harness::new("prng.p63", "RAND_SEED8");
        h.cpu.registers.set_a(seed);
        assert_eq!(h.call().registers.a(), x8(effective), "RAND_SEED8(${seed:02X})");
        // the next RAND8 carries on from there
        let mut both = Harness::calling("prng.p63", &["RAND_SEED8", "RAND8"]);
        both.cpu.registers.set_a(seed);
        assert_eq!(both.call().registers.a(), x8(x8(effective)), "RAND8 after seeding ${seed:02X}");
    }
    for (seed, effective) in [(0x8F52u16, 0x8F52u16), (0, 0xFFF1)] {
        let mut h = Harness::new("prng.p63", "RAND_SEED16");
        h.cpu.registers.set_d(seed);
        assert_eq!(h.call().registers.d(), x16(effective), "RAND_SEED16(${seed:04X})");
    }
    for (seed, effective) in [(0x8DB1_8F52u32, 0x8DB1_8F52u32), (0, 0xFFFF_FFFB)] {
        let mut h = Harness::new("prng.p63", "RAND_SEED32");
        h.cpu.registers.set_q(seed);
        assert_eq!(h.call().registers.q(), x32(effective), "RAND_SEED32(${seed:08X})");
    }
}

/// Calls one trig routine many times on one loaded program.
struct Trig(Harness);

impl Trig {
    fn new(routine: &str) -> Self {
        Self(Harness::new("trig.p63", routine))
    }

    fn a(&mut self, angle: u8) -> i8 {
        self.0.cpu.registers.set_a(angle);
        self.0.call().registers.a() as i8
    }

    fn d(&mut self, angle: u16) -> i16 {
        self.0.cpu.registers.set_d(angle);
        self.0.call().registers.d() as i16
    }
}

fn ratio8(a: i32, b: i32) -> i8 {
    let q = (a.abs() * 110 / b.abs()).min(127);
    (if (a < 0) != (b < 0) { -q } else { q }) as i8
}

fn ratio16(a: i32, b: i32) -> i16 {
    let q = if a.abs() >= b.abs() { 32767 } else { a.abs() * 32768 / b.abs() };
    (if (a < 0) != (b < 0) { -q } else { q }) as i16
}

#[test]
fn trig_8bit_sin_cos_exact() {
    if !have_utils() {
        return;
    }
    let (mut sin, mut cos) = (Trig::new("SIN"), Trig::new("COS"));
    for a in 0..=255u8 {
        let rad = f64::from(a) * std::f64::consts::PI / 128.0;
        assert_eq!(i32::from(sin.a(a)), (110.0 * rad.sin()).round() as i32, "SIN({a})");
        assert_eq!(i32::from(cos.a(a)), (110.0 * rad.cos()).round() as i32, "COS({a})");
    }
}

#[test]
fn trig_8bit_ratios_exact() {
    if !have_utils() {
        return;
    }
    let (mut sin, mut cos) = (Trig::new("SIN"), Trig::new("COS"));
    let mut tan = Trig::new("TAN");
    let (mut cosec, mut sec, mut cotan) = (Trig::new("COSEC"), Trig::new("SEC"), Trig::new("COTAN"));
    for a in 0..=255u8 {
        let (s, c) = (i32::from(sin.a(a)), i32::from(cos.a(a)));
        let undef = 127;
        assert_eq!(tan.a(a), if c == 0 { undef } else { ratio8(s, c) }, "TAN({a})");
        assert_eq!(cosec.a(a), if s == 0 { undef } else { ratio8(110, s) }, "COSEC({a})");
        assert_eq!(sec.a(a), if c == 0 { undef } else { ratio8(110, c) }, "SEC({a})");
        assert_eq!(cotan.a(a), if s == 0 { undef } else { ratio8(c, s) }, "COTAN({a})");
    }
}

#[test]
fn trig_16bit_sin_cos_close() {
    if !have_utils() {
        return;
    }
    let (mut sin, mut cos) = (Trig::new("SIN16"), Trig::new("COS16"));
    let mut worst = 0;
    for step in 0..2048u32 {
        let a = (step * 32 + step % 31) as u16;
        let rad = f64::from(a) * std::f64::consts::PI / 32768.0;
        let es = (i32::from(sin.d(a)) - (32767.0 * rad.sin()).round() as i32).abs();
        let ec = (i32::from(cos.d(a)) - (32767.0 * rad.cos()).round() as i32).abs();
        worst = worst.max(es).max(ec);
        assert!(es <= 4 && ec <= 4, "angle {a}: SIN16 off by {es}, COS16 off by {ec}");
    }
    println!("SIN16/COS16 worst error: {worst}");
}

#[test]
fn trig_16bit_ratios_exact() {
    if !have_utils() {
        return;
    }
    let (mut sin, mut cos) = (Trig::new("SIN16"), Trig::new("COS16"));
    let mut tan = Trig::new("TAN16");
    let (mut cosec, mut sec, mut cotan) = (Trig::new("COSEC16"), Trig::new("SEC16"), Trig::new("COTAN16"));
    for step in 0..512u32 {
        let a = (step * 128 + step % 97) as u16;
        let (s, c) = (i32::from(sin.d(a)), i32::from(cos.d(a)));
        let undef = 32767;
        assert_eq!(tan.d(a), if c == 0 { undef } else { ratio16(s, c) }, "TAN16({a})");
        assert_eq!(cosec.d(a), if s == 0 { undef } else { ratio16(32767, s) }, "COSEC16({a})");
        assert_eq!(sec.d(a), if c == 0 { undef } else { ratio16(32767, c) }, "SEC16({a})");
        assert_eq!(cotan.d(a), if s == 0 { undef } else { ratio16(c, s) }, "COTAN16({a})");
    }
}

#[test]
fn trig_documented_examples() {
    if !have_utils() {
        return;
    }
    assert_eq!(Trig::new("COS").a(0), 110);
    assert_eq!(Trig::new("SIN").a(64), 110);
    assert_eq!(Trig::new("TAN").a(32), 110);
    assert!((Trig::new("TAN").a(16) - 45).abs() <= 1);
    assert_eq!(Trig::new("COSEC").a(64), 110);
    assert_eq!(Trig::new("SEC").a(0), 110);
    assert_eq!(Trig::new("COTAN").a(32), 110);
    assert_eq!(Trig::new("COS16").d(0), 32767);
    assert_eq!(Trig::new("SIN16").d(16384), 32767);
    assert!((Trig::new("SIN16").d(8192) - 23170).abs() <= 1);
    assert_eq!(Trig::new("TAN16").d(8192), 32767);
    assert!((Trig::new("TAN16").d(4096) - 13573).abs() <= 1);
    assert_eq!(Trig::new("COSEC16").d(16384), 32767);
    assert_eq!(Trig::new("SEC16").d(0), 32767);
    assert_eq!(Trig::new("COTAN16").d(8192), 32767);
}

#[test]
fn crc16_check_value() {
    if !have_utils() {
        return;
    }
    let mut h = Harness::new("crc16.p63", "CRC16_BLOCK");
    let data = b"123456789";
    for (i, b) in data.iter().enumerate() {
        h.cpu.bus.write(Addr::new(0x4000 + i as u32), *b);
    }
    h.cpu.registers.set_w(0xFFFF);
    h.cpu.registers.set_y(0x4000);
    h.cpu.registers.set_x(data.len() as u32);
    let cpu = h.call();
    assert_eq!(cpu.registers.w(), 0x29B1, "CRC-16/CCITT-FALSE of \"123456789\"");
    assert_eq!(cpu.registers.y(), 0x4000 + 9);
}

#[test]
fn crc16_long_block_matches_rust() {
    if !have_utils() {
        return;
    }
    fn crc(data: &[u8]) -> u16 {
        let mut c = 0xFFFFu16;
        for &b in data {
            c ^= u16::from(b) << 8;
            for _ in 0..8 {
                c = if c & 0x8000 != 0 { (c << 1) ^ 0x1021 } else { c << 1 };
            }
        }
        c
    }
    let data: Vec<u8> = (0..700u32).map(|i| (i * 7 + 3) as u8).collect();
    let mut h = Harness::new("crc16.p63", "CRC16_BLOCK");
    for (i, b) in data.iter().enumerate() {
        h.cpu.bus.write(Addr::new(0x4000 + i as u32), *b);
    }
    h.cpu.registers.set_w(0xFFFF);
    h.cpu.registers.set_y(0x4000);
    h.cpu.registers.set_x(data.len() as u32);
    assert_eq!(h.call().registers.w(), crc(&data), "700 bytes crosses the 256-byte block path");
}

#[test]
fn prng_sequences_match_reference_xorshift() {
    if !have_utils() {
        return;
    }
    let mut s8 = 0xFBu8;
    let mut s16 = 0xFFF1u16;
    let mut s32 = 0xFFFF_FFFBu32;
    let mut h8 = Harness::new("prng.p63", "RAND8");
    let mut h16 = Harness::new("prng.p63", "RAND16");
    let mut h32 = Harness::new("prng.p63", "RAND32");
    for i in 0..20 {
        s8 ^= s8 << 1;
        s8 ^= s8 >> 7;
        s8 ^= s8 << 3;
        s16 ^= s16 << 1;
        s16 ^= s16 >> 5;
        s16 ^= s16 << 2;
        s32 ^= s32 << 13;
        s32 ^= s32 >> 17;
        s32 ^= s32 << 5;
        assert_eq!(h8.call().registers.a(), s8, "RAND8 call {i}");
        assert_eq!(h16.call().registers.d(), s16, "RAND16 call {i}");
        assert_eq!(h32.call().registers.q(), s32, "RAND32 call {i}");
    }
}

#[test]
fn every_util_file_assembles_or_reports_known_problems() {
    if !have_utils() {
        return;
    }
    let mut report = Vec::new();
    for name in ["bitops", "crc16", "math", "prng", "trig"] {
        match p63_asm::assemble_file(format!("{UTILS}/{name}.p63")) {
            Ok(_) => report.push(format!("{name}: ok")),
            Err(errors) => report.extend(errors.iter().map(ToString::to_string)),
        }
    }
    println!("{}", report.join("\n"));
    for name in ["bitops", "crc16", "math", "prng", "trig"] {
        assert!(report.contains(&format!("{name}: ok")), "{name} should assemble:\n{}", report.join("\n"));
    }
}
