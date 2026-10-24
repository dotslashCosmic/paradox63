//! p63-spec: Paradox63 hardware constants and tables.

#[allow(clippy::all, clippy::pedantic)]
mod generated;

pub use generated::*;

/// Base cycle range `(min, max)` for an opcode, parsed from the table text.
#[must_use]
pub fn base_cycles(opcode: u8) -> (u32, u32) {
    let text = OPCODES[opcode as usize].cycles;
    let lead = |s: &str| -> u32 {
        s.chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .unwrap_or(0)
    };
    let min = lead(text);
    match text.split_once('-') {
        Some((_, hi)) if !text.contains('/') => (min, lead(hi)),
        _ => (min, min),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_version_is_pinned() {
        assert_eq!(SPEC_VERSION, "0.11.0");
    }

    #[test]
    fn opcode_table_is_complete() {
        assert_eq!(OPCODES.len(), 256);
        for (i, op) in OPCODES.iter().enumerate() {
            assert_eq!(op.opcode as usize, i);
        }
        assert_eq!(OPCODES.iter().filter(|o| o.legal).count(), 166);
    }

    #[test]
    fn regions_sorted_and_disjoint() {
        for r in &REGIONS {
            assert!(r.base <= r.end, "{}", r.id);
            assert!(r.end <= 0xFF_FFFF, "{}", r.id);
        }
        let mut sorted: Vec<_> = REGIONS.iter().collect();
        sorted.sort_by_key(|r| r.base);
        for w in sorted.windows(2) {
            assert!(w[0].end < w[1].base, "{} overlaps {}", w[0].id, w[1].id);
        }
    }

    #[test]
    fn base_cycles_parses_all_forms() {
        for op in 0..=255u8 {
            let (lo, hi) = base_cycles(op);
            assert!(lo <= hi, "opcode {op:#04X}");
        }
        assert_eq!(base_cycles(0x01), (2, 2));
    }
}
