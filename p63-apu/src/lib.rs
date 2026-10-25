//! p63-apu: the Paradox63 audio engine: 8 sample voices with ADSR envelopes, mixed to stereo.

use p63_spec as spec;

/// Output samples per second (CLK / 224).
pub const SAMPLE_RATE: u32 = spec::CLOCK_HZ / spec::AUDIO_SAMPLE_DIV;
/// CPU cycles per output tick.
pub const TICK_CYCLES: u32 = spec::AUDIO_SAMPLE_DIV;

const VOICES: usize = spec::AUD_VOICES as usize;

const fn reg(addr: u32) -> usize {
    (addr - spec::AUDIO_IO_BASE) as usize
}

const fn voice_reg(addr: u32) -> usize {
    (addr - spec::AUD_V0_VOL_L) as usize
}

const MVOL_L: usize = reg(spec::AUD_MVOL_L);
const MVOL_R: usize = reg(spec::AUD_MVOL_R);
const KON: usize = reg(spec::AUD_KON);
const KOFF: usize = reg(spec::AUD_KOFF);
const STAT: usize = reg(spec::AUD_STAT);
const ENDX: usize = reg(spec::AUD_ENDX);
const IRQEN: usize = reg(spec::AUD_IRQEN);
const DIR: usize = reg(spec::AUD_DIR);
const NOISE: usize = reg(spec::AUD_NOISE);
const ACTRL: usize = reg(spec::AUD_ACTRL);
const ENVX: usize = reg(spec::AUD_ENVX);

const VOL_L: usize = voice_reg(spec::AUD_V0_VOL_L);
const VOL_R: usize = voice_reg(spec::AUD_V0_VOL_R);
const PITCH_L: usize = voice_reg(spec::AUD_V0_PITCH_L);
const SRC: usize = voice_reg(spec::AUD_V0_SRC);
const ADSR1: usize = voice_reg(spec::AUD_V0_ADSR1);
const ADSR2: usize = voice_reg(spec::AUD_V0_ADSR2);
const CTRL: usize = voice_reg(spec::AUD_V0_CTRL);

const CTRL_LOOP: u8 = 0x01;
const CTRL_NOISE: u8 = 0x02;
pub const ACTRL_ENABLE: u8 = 0x01;
pub const ACTRL_RESET: u8 = 0x02;
pub const ACTRL_LINEAR: u8 = 0x04;

/// Full envelope level in 8.8 fixed point.
const ENV_MAX: u32 = 0xFF00;
/// Fraction bits of a voice's sample position (4.12 pitch).
const FRAC: u32 = 12;
const LFSR_SEED: u16 = 0x4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Silent,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Debug, Clone, Copy, Default)]
struct Voice {
    phase: Phase,
    /// envelope level, 8.8
    level: u32,
    /// Audio RAM address with 12 fraction bits
    pos: u32,
    loop_at: u32,
    end: u32,
}

/// The audio engine: registers, voice state and the noise generator.
#[derive(Debug, Clone)]
pub struct Apu {
    regs: [u8; 128],
    voices: [Voice; VOICES],
    endx: u8,
    /// KON/KOFF bits waiting for the next tick
    key_on: u8,
    key_off: u8,
    lfsr: u16,
    noise_count: u32,
}

impl Default for Apu {
    fn default() -> Self {
        Self {
            regs: [0; 128],
            voices: [Voice::default(); VOICES],
            endx: 0,
            key_on: 0,
            key_off: 0,
            lfsr: LFSR_SEED,
            noise_count: 0,
        }
    }
}

fn aram_byte(aram: &[u8], addr: u32) -> i32 {
    i32::from(aram.get((addr & 0xFFFF) as usize).copied().unwrap_or(0) as i8)
}

fn u16_at(aram: &[u8], addr: u32) -> u32 {
    u32::from(aram_byte(aram, addr) as u8) | u32::from(aram_byte(aram, addr + 1) as u8) << 8
}

impl Apu {
    fn vreg(&self, n: usize, r: usize) -> u8 {
        self.regs[n * spec::AUD_STRIDE as usize + r]
    }

    /// Register read; `off` is from the start of the Audio I/O block.
    #[must_use]
    pub fn read(&self, off: u32) -> u8 {
        let off = off as usize;
        match off {
            KON | KOFF => 0,
            STAT => self.playing(),
            ENDX => self.endx,
            ACTRL => self.regs[ACTRL] & !ACTRL_RESET,
            _ if (ENVX..ENVX + VOICES).contains(&off) => (self.voices[off - ENVX].level >> 8) as u8,
            _ => self.regs[off],
        }
    }

    /// Register write; `off` is from the start of the Audio I/O block.
    pub fn write(&mut self, off: u32, value: u8) {
        let off = off as usize;
        match off {
            KON => self.key_on |= value,
            KOFF => self.key_off |= value,
            STAT => {}
            ENDX => self.endx &= !value,
            ACTRL => {
                if value & ACTRL_RESET != 0 {
                    self.voices = [Voice::default(); VOICES];
                    self.endx = 0;
                    self.key_on = 0;
                    self.key_off = 0;
                }
                self.regs[ACTRL] = value & !ACTRL_RESET;
            }
            _ if (ENVX..ENVX + VOICES).contains(&off) => {}
            _ => self.regs[off] = value,
        }
    }

    /// Bit n set while voice n is playing.
    fn playing(&self) -> u8 {
        (0..VOICES).filter(|&n| self.voices[n].phase != Phase::Silent).fold(0, |bits, n| bits | 1 << n)
    }

    /// True while an enabled ENDX bit holds `/FIRQ` low.
    #[must_use]
    pub fn firq(&self) -> bool {
        self.endx & self.regs[IRQEN] != 0
    }

    fn start(&mut self, n: usize, aram: &[u8]) {
        let entry = (u32::from(self.regs[DIR]) << 8) + u32::from(self.vreg(n, SRC)) * 8;
        let end = match u16_at(aram, entry + 4) {
            0 => 0x1_0000, // END is one past the last sample, so 0 means the top of Audio RAM
            e => e,
        };
        self.voices[n] = Voice {
            phase: Phase::Attack,
            level: 0,
            pos: u16_at(aram, entry) << FRAC,
            loop_at: u16_at(aram, entry + 2),
            end,
        };
    }

    /// Moves voice n's envelope one tick along.
    fn envelope(&mut self, n: usize) {
        let (adsr1, adsr2) = (self.vreg(n, ADSR1), self.vreg(n, ADSR2));
        let step = |rate: u8| u32::from(spec::ENV_STEPS[usize::from(rate & 15)]);
        let sustain = (u32::from(adsr2 >> 4) + 1) * 16 - 1;
        let v = &mut self.voices[n];
        match v.phase {
            Phase::Silent | Phase::Sustain => {}
            Phase::Attack => {
                v.level = (v.level + step(adsr1 >> 4)).min(ENV_MAX);
                if v.level == ENV_MAX {
                    v.phase = Phase::Decay;
                }
            }
            Phase::Decay => {
                v.level = v.level.saturating_sub(step(adsr1)).max(sustain << 8);
                if v.level == sustain << 8 {
                    v.phase = Phase::Sustain;
                }
            }
            Phase::Release => {
                v.level = v.level.saturating_sub(step(adsr2));
                if v.level == 0 {
                    v.phase = Phase::Silent;
                }
            }
        }
    }

    /// Voice n's current sample, then steps its position.
    fn sample(&mut self, n: usize, aram: &[u8]) -> i32 {
        if self.vreg(n, CTRL) & CTRL_NOISE != 0 {
            return i32::from((self.lfsr >> 7) as u8 as i8);
        }
        let pitch = u32::from(u16::from_le_bytes([self.vreg(n, PITCH_L), self.vreg(n, PITCH_L + 1)]));
        let looping = self.vreg(n, CTRL) & CTRL_LOOP != 0;
        let linear = self.regs[ACTRL] & ACTRL_LINEAR != 0;
        let v = &mut self.voices[n];
        let addr = v.pos >> FRAC;
        let s0 = aram_byte(aram, addr);
        let s = if linear {
            let frac = (v.pos & ((1 << FRAC) - 1)) as i32;
            s0 + (((aram_byte(aram, addr + 1) - s0) * frac) >> FRAC)
        } else {
            s0
        };

        v.pos += pitch;
        if v.pos >> FRAC >= v.end {
            self.endx |= 1 << n;
            if looping && v.loop_at < v.end {
                let len = v.end - v.loop_at;
                let past = (v.pos >> FRAC) - v.loop_at;
                v.pos = (v.loop_at + past % len) << FRAC | (v.pos & ((1 << FRAC) - 1));
            } else if looping {
                v.pos = v.loop_at << FRAC;
            } else {
                *v = Voice::default();
            }
        }
        s
    }

    fn clock_noise(&mut self) {
        let rate = u32::from(self.regs[NOISE] & 31);
        if rate == 0 {
            return;
        }
        self.noise_count += 1;
        if self.noise_count >= 32 - rate {
            self.noise_count = 0;
            let bit = (self.lfsr ^ (self.lfsr >> 1)) & 1;
            self.lfsr = (self.lfsr >> 1) | bit << 14;
        }
    }

    /// Runs one output tick and returns the stereo sample.
    /// `aram` is the 64 KB Audio RAM.
    pub fn tick(&mut self, aram: &[u8]) -> (i16, i16) {
        if self.regs[ACTRL] & ACTRL_ENABLE == 0 {
            return (0, 0);
        }
        for n in 0..VOICES {
            if self.key_on & 1 << n != 0 {
                self.start(n, aram);
            }
            if self.key_off & 1 << n != 0 && self.voices[n].phase != Phase::Silent {
                self.voices[n].phase = Phase::Release;
            }
        }
        (self.key_on, self.key_off) = (0, 0);
        self.clock_noise();

        let (mut acc_l, mut acc_r) = (0i32, 0i32);
        for n in 0..VOICES {
            self.envelope(n);
            if self.voices[n].phase == Phase::Silent {
                continue;
            }
            let env = (self.voices[n].level >> 8) as i32;
            let s = self.sample(n, aram);
            acc_l += (s * env * i32::from(self.vreg(n, VOL_L))) >> 8;
            acc_r += (s * env * i32::from(self.vreg(n, VOL_R))) >> 8;
        }
        let out = |acc: i32, mvol: u8| ((acc * i32::from(mvol)) >> 11).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        (out(acc_l, self.regs[MVOL_L]), out(acc_r, self.regs[MVOL_R]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARAM: usize = 0x1_0000;
    const DIR_PAGE: u8 = 0x20;

    fn w(apu: &mut Apu, addr: u32, value: u8) {
        apu.write(addr - spec::AUDIO_IO_BASE, value);
    }

    fn r(apu: &Apu, addr: u32) -> u8 {
        apu.read(addr - spec::AUDIO_IO_BASE)
    }

    fn voice(n: u32, addr: u32) -> u32 {
        addr + n * spec::AUD_STRIDE
    }

    /// Sample `n` in the directory: start, loop, end.
    fn put_sample(aram: &mut [u8], n: usize, start: u16, loop_at: u16, end: u16) {
        let at = (usize::from(DIR_PAGE) << 8) + n * 8;
        aram[at..at + 6].copy_from_slice(&[start, loop_at, end].map(u16::to_le_bytes).concat());
    }

    /// Engine on, full master volume, voice 0 at full volume, instant attack, full sustain.
    fn setup(aram: &mut [u8], data: &[i8], pitch: u16, ctrl: u8) -> Apu {
        let mut apu = Apu::default();
        for (i, &s) in data.iter().enumerate() {
            aram[0x100 + i] = s as u8;
        }
        put_sample(aram, 0, 0x100, 0x100, 0x100 + data.len() as u16);
        w(&mut apu, spec::AUD_ACTRL, ACTRL_ENABLE);
        w(&mut apu, spec::AUD_MVOL_L, 255);
        w(&mut apu, spec::AUD_MVOL_R, 255);
        w(&mut apu, spec::AUD_DIR, DIR_PAGE);
        w(&mut apu, spec::AUD_V0_VOL_L, 255);
        w(&mut apu, spec::AUD_V0_VOL_R, 255);
        w(&mut apu, spec::AUD_V0_PITCH_L, pitch as u8);
        w(&mut apu, spec::AUD_V0_PITCH_H, (pitch >> 8) as u8);
        w(&mut apu, spec::AUD_V0_ADSR1, 0xF0);
        w(&mut apu, spec::AUD_V0_ADSR2, 0xF0);
        w(&mut apu, spec::AUD_V0_CTRL, ctrl);
        w(&mut apu, spec::AUD_KON, 0x01);
        apu
    }

    /// One voice at full envelope, volume and master volume.
    fn full(s: i32) -> i16 {
        ((((s * 255 * 255) >> 8) * 255) >> 11) as i16
    }

    fn left(apu: &mut Apu, aram: &[u8], ticks: usize) -> Vec<i16> {
        (0..ticks).map(|_| apu.tick(aram).0).collect()
    }

    #[test]
    fn native_pitch_plays_each_sample_once() {
        let mut aram = vec![0; ARAM];
        let mut apu = setup(&mut aram, &[64, -64, 127, -128], spec::AUD_PITCH_1_0 as u16, 0);
        assert_eq!(left(&mut apu, &aram, 4), [full(64), full(-64), full(127), full(-128)]);
        assert_eq!(full(64), 2024);
    }

    #[test]
    fn pitch_skips_and_repeats() {
        let mut aram = vec![0; ARAM];
        let data = [10, 20, 30, 40];
        let mut apu = setup(&mut aram, &data, 0x2000, CTRL_LOOP);
        assert_eq!(left(&mut apu, &aram, 4), [full(10), full(30), full(10), full(30)], "octave up");
        let mut apu = setup(&mut aram, &data, 0x0800, 0);
        assert_eq!(left(&mut apu, &aram, 4), [full(10), full(10), full(20), full(20)], "octave down");
    }

    #[test]
    fn linear_interpolation_blends_neighbours() {
        let mut aram = vec![0; ARAM];
        let mut apu = setup(&mut aram, &[0, 64, 64], 0x0800, 0);
        w(&mut apu, spec::AUD_ACTRL, ACTRL_ENABLE | ACTRL_LINEAR);
        assert_eq!(left(&mut apu, &aram, 3), [full(0), full(32), full(64)]);
    }

    #[test]
    fn end_sets_endx_and_loops_or_stops() {
        let mut aram = vec![0; ARAM];
        let mut apu = setup(&mut aram, &[1, 2, 3], spec::AUD_PITCH_1_0 as u16, CTRL_LOOP);
        put_sample(&mut aram, 0, 0x100, 0x101, 0x103);
        w(&mut apu, spec::AUD_KON, 0x01);
        assert_eq!(left(&mut apu, &aram, 2), [full(1), full(2)]);
        assert_eq!(r(&apu, spec::AUD_ENDX), 0);
        assert_eq!(left(&mut apu, &aram, 3), [full(3), full(2), full(3)], "loops back to sample 1");
        assert_eq!(r(&apu, spec::AUD_ENDX), 0x01);
        w(&mut apu, spec::AUD_ENDX, 0x01);
        assert_eq!(r(&apu, spec::AUD_ENDX), 0, "write 1 to clear");

        let mut apu = setup(&mut aram, &[1, 2, 3], spec::AUD_PITCH_1_0 as u16, 0);
        assert_eq!(left(&mut apu, &aram, 4), [full(1), full(2), full(3), 0]);
        assert_eq!(r(&apu, spec::AUD_STAT), 0, "silent after a one-shot");
        assert_eq!(r(&apu, spec::AUD_ENDX), 0x01);
    }

    #[test]
    fn endx_raises_firq_only_when_enabled() {
        let mut aram = vec![0; ARAM];
        let mut apu = setup(&mut aram, &[1], spec::AUD_PITCH_1_0 as u16, 0);
        apu.tick(&aram);
        assert!(!apu.firq());
        w(&mut apu, spec::AUD_IRQEN, 0x01);
        assert!(apu.firq());
        w(&mut apu, spec::AUD_ENDX, 0x01);
        assert!(!apu.firq());
    }

    #[test]
    fn adsr_follows_the_rate_table() {
        let mut aram = vec![0; ARAM];
        let mut apu = setup(&mut aram, &[0; 64], spec::AUD_PITCH_1_0 as u16, CTRL_LOOP);
        w(&mut apu, spec::AUD_V0_ADSR1, 0xEE); // attack 14, decay 14 (256 per tick)
        w(&mut apu, spec::AUD_V0_ADSR2, 0x7D); // sustain 7 = level 127, release 13 (128 per tick)
        w(&mut apu, spec::AUD_KON, 0x01);
        let envx = |apu: &Apu| r(apu, spec::AUD_ENVX);
        apu.tick(&aram);
        assert_eq!(envx(&apu), 1, "attack adds 256 per tick");
        for _ in 0..254 {
            apu.tick(&aram);
        }
        assert_eq!(envx(&apu), 255, "full after 255 ticks");
        for _ in 0..128 {
            apu.tick(&aram);
        }
        assert_eq!(envx(&apu), 127, "decayed to the sustain level");
        for _ in 0..100 {
            apu.tick(&aram);
        }
        assert_eq!(envx(&apu), 127, "sustain holds");
        w(&mut apu, spec::AUD_KOFF, 0x01);
        assert_eq!(r(&apu, spec::AUD_STAT), 1);
        for _ in 0..253 {
            apu.tick(&aram);
        }
        assert_eq!(envx(&apu), 0);
        assert_eq!(r(&apu, spec::AUD_STAT), 1, "still releasing");
        apu.tick(&aram);
        assert_eq!(r(&apu, spec::AUD_STAT), 0, "release ends silent");
    }

    #[test]
    fn attack_rate_zero_never_starts() {
        let mut aram = vec![0; ARAM];
        let mut apu = setup(&mut aram, &[100; 4], spec::AUD_PITCH_1_0 as u16, CTRL_LOOP);
        w(&mut apu, spec::AUD_V0_ADSR1, 0x0F);
        w(&mut apu, spec::AUD_KON, 0x01);
        assert_eq!(left(&mut apu, &aram, 10), [0; 10]);
        assert_eq!(r(&apu, spec::AUD_STAT), 1, "keyed on, so playing");
    }

    #[test]
    fn volumes_pan_and_scale() {
        let mut aram = vec![0; ARAM];
        let mut apu = setup(&mut aram, &[100; 4], spec::AUD_PITCH_1_0 as u16, CTRL_LOOP);
        w(&mut apu, spec::AUD_V0_VOL_R, 0);
        w(&mut apu, spec::AUD_MVOL_L, 128);
        let (l, r) = apu.tick(&aram);
        assert_eq!((l, r), (((((100 * 255 * 255) >> 8) * 128) >> 11) as i16, 0));
    }

    #[test]
    fn voices_add_up() {
        let mut aram = vec![0; ARAM];
        let mut apu = setup(&mut aram, &[-128; 4], spec::AUD_PITCH_1_0 as u16, CTRL_LOOP);
        for n in 1..8 {
            for (a, v) in [(spec::AUD_V0_VOL_L, 255), (spec::AUD_V0_PITCH_H, 0x10), (spec::AUD_V0_ADSR1, 0xF0), (spec::AUD_V0_ADSR2, 0xF0), (spec::AUD_V0_CTRL, CTRL_LOOP)] {
                w(&mut apu, voice(n, a), v);
            }
        }
        w(&mut apu, spec::AUD_KON, 0xFF);
        let (l, _) = apu.tick(&aram);
        assert_eq!(l, (((8 * ((-128 * 255 * 255) >> 8)) * 255) >> 11) as i16, "eight voices at full scale don't clip");
        assert_eq!(r(&apu, spec::AUD_STAT), 0xFF);
    }

    #[test]
    fn noise_replaces_the_sample() {
        let mut aram = vec![0; ARAM];
        let mut apu = setup(&mut aram, &[0; 4], 0, CTRL_NOISE);
        let still = left(&mut apu, &aram, 8);
        assert!(still.windows(2).all(|p| p[0] == p[1]), "rate 0 stops the LFSR");
        assert_ne!(still[0], 0, "noise ignores SRC and PITCH");
        w(&mut apu, spec::AUD_NOISE, 31);
        let moving = left(&mut apu, &aram, 64);
        assert!(moving.windows(2).any(|p| p[0] != p[1]));
    }

    #[test]
    fn disable_mutes_and_reset_silences() {
        let mut aram = vec![0; ARAM];
        let mut apu = setup(&mut aram, &[50; 4], spec::AUD_PITCH_1_0 as u16, CTRL_LOOP);
        apu.tick(&aram);
        w(&mut apu, spec::AUD_ACTRL, 0);
        assert_eq!(apu.tick(&aram), (0, 0));
        w(&mut apu, spec::AUD_ACTRL, ACTRL_ENABLE | ACTRL_RESET);
        assert_eq!(r(&apu, spec::AUD_ACTRL), ACTRL_ENABLE, "RESET clears itself");
        assert_eq!(r(&apu, spec::AUD_STAT), 0);
        assert_eq!(apu.tick(&aram), (0, 0));
    }

    #[test]
    fn rate_is_clk_over_224() {
        assert_eq!(SAMPLE_RATE, 31_960);
    }
}
