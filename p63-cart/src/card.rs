//! Memory cards: 1 MB in a slot beside the cartridge, and the layout every program
//! keeps on it so that saves from different games can share a card.
//!
//! Block 0 is the directory: `P63M`, the layout version, then one 32-byte entry for each
//! of blocks 1-127. A save is a chain of blocks; its first block's entry names the game
//! and the save, and the first 256 bytes of its data are a header the BIOS shows.

use p63_spec as spec;

pub const SIZE: usize = spec::CARD_SIZE as usize;
pub const BLOCK: usize = spec::CARD_BLOCK as usize;
pub const BLOCKS: usize = spec::CARD_BLOCKS as usize;
pub const MAGIC: &[u8; 4] = b"P63M";

const ENTRY_SIZE: usize = spec::CE_SIZE as usize;
const FIRST: u8 = spec::CES_FIRST as u8;
const MORE: u8 = spec::CES_MORE as u8;

/// Where block `b`'s directory entry starts (blocks 1 to 127).
fn entry(b: usize) -> usize {
    spec::CARD_ENTRIES as usize + (b - 1) * ENTRY_SIZE
}

fn text(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn field(out: &mut [u8], s: &str) {
    out.fill(0);
    let n = s.len().min(out.len());
    out[..n].copy_from_slice(&s.as_bytes()[..n]);
}

/// A new card: an empty directory, and erased ($FF) blocks.
#[must_use]
pub fn blank() -> Vec<u8> {
    let mut card = vec![0xFF; SIZE];
    card[..BLOCK].fill(0);
    card[..4].copy_from_slice(MAGIC);
    card[4] = spec::CARD_VERSION as u8;
    card
}

#[must_use]
pub fn is_formatted(card: &[u8]) -> bool {
    card.len() == SIZE && &card[..4] == MAGIC && card[4] == spec::CARD_VERSION as u8
}

/// One save, as the directory describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Save {
    pub first: usize,
    pub blocks: Vec<usize>,
    pub seq: u8,
    pub game: String,
    pub name: String,
    /// The title from the save's header, as the BIOS shows it.
    pub title: String,
}

/// The blocks of the chain starting at `first`, stopping at a loop or a bad link.
fn chain(card: &[u8], first: usize) -> Vec<usize> {
    let mut blocks = vec![first];
    let mut b = card[entry(first) + spec::CE_NEXT as usize] as usize;
    while (1..BLOCKS).contains(&b) && !blocks.contains(&b) && card[entry(b)] == MORE {
        blocks.push(b);
        b = card[entry(b) + spec::CE_NEXT as usize] as usize;
    }
    blocks
}

/// Every save on the card, in directory order; `None` if the card isn't formatted.
#[must_use]
pub fn saves(card: &[u8]) -> Option<Vec<Save>> {
    if !is_formatted(card) {
        return None;
    }
    let found = (1..BLOCKS)
        .filter(|&b| card[entry(b)] == FIRST)
        .map(|first| {
            let e = &card[entry(first)..entry(first) + ENTRY_SIZE];
            let at = first * BLOCK;
            Save {
                first,
                blocks: chain(card, first),
                seq: e[spec::CE_SEQ as usize],
                game: text(&e[spec::CE_GAME as usize..spec::CE_GAME as usize + 16]),
                name: text(&e[spec::CE_NAME as usize..spec::CE_NAME as usize + 12]),
                title: text(&card[at + spec::SH_TITLE as usize..at + spec::SH_TITLE as usize + 32]),
            }
        })
        .collect();
    Some(found)
}

/// Blocks no save's chain reaches. A save cut off half-written leaves its blocks marked
/// but unreachable, so they count as free.
#[must_use]
pub fn free_blocks(card: &[u8]) -> Vec<usize> {
    let used: Vec<usize> = saves(card).unwrap_or_default().into_iter().flat_map(|s| s.blocks).collect();
    (1..BLOCKS).filter(|b| !used.contains(b)).collect()
}

/// Writes a save the way a program on the console does: the data blocks and their
/// entries first, the first block's state last, then the old copy is freed.
/// `data` starts with the 256-byte save header.
pub fn write_save(card: &mut [u8], game: &str, name: &str, data: &[u8]) -> Result<(), String> {
    let old: Vec<Save> = saves(card).ok_or("the card isn't formatted")?.into_iter().filter(|s| s.game == game && s.name == name).collect();
    let need = data.len().div_ceil(BLOCK).max(1);
    let free = free_blocks(card);
    if free.len() < need {
        return Err(format!("{need} blocks needed, {} free", free.len()));
    }
    let blocks = &free[..need];
    let seq = old.iter().map(|s| s.seq).max().map_or(0, |s| s.wrapping_add(1));
    for (i, &b) in blocks.iter().enumerate() {
        let chunk = &data[(i * BLOCK).min(data.len())..((i + 1) * BLOCK).min(data.len())];
        card[b * BLOCK..b * BLOCK + chunk.len()].copy_from_slice(chunk);
        let e = entry(b);
        card[e..e + ENTRY_SIZE].fill(0);
        card[e + 1] = blocks.get(i + 1).map_or(0, |&n| n as u8);
        if i > 0 {
            card[e] = MORE;
        }
    }
    let e = entry(blocks[0]);
    card[e + spec::CE_COUNT as usize] = need as u8;
    card[e + spec::CE_SEQ as usize] = seq;
    field(&mut card[e + spec::CE_GAME as usize..e + spec::CE_GAME as usize + 16], game);
    field(&mut card[e + spec::CE_NAME as usize..e + spec::CE_NAME as usize + 12], name);
    card[e] = FIRST;
    for s in old {
        delete(card, &s);
    }
    Ok(())
}

/// Frees a save's blocks: its first entry first, so a cut-off delete leaves no half save.
pub fn delete(card: &mut [u8], save: &Save) {
    for &b in &save.blocks {
        card[entry(b)] = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(title: &str, len: usize) -> Vec<u8> {
        let mut d = vec![0x5A; len.max(spec::SH_DATA as usize)];
        field(&mut d[..32], title);
        d
    }

    #[test]
    fn a_blank_card_is_empty() {
        let card = blank();
        assert!(is_formatted(&card));
        assert_eq!(saves(&card).unwrap(), Vec::new());
        assert_eq!(free_blocks(&card).len(), BLOCKS - 1);
        assert!(saves(&vec![0xFF; SIZE]).is_none(), "an erased card isn't formatted");
    }

    #[test]
    fn saves_chain_and_replace() {
        let mut card = blank();
        write_save(&mut card, "PONG", "SCORES", &data("Pong high scores", 100)).unwrap();
        write_save(&mut card, "GAMETITLE", "SLOT1", &data("PlayerName, day 3", 3 * BLOCK + 1)).unwrap();
        let s = saves(&card).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!((s[1].game.as_str(), s[1].name.as_str(), s[1].blocks.len()), ("GAMETITLE", "SLOT1", 4));
        assert_eq!(s[1].title, "PlayerName, day 3");
        write_save(&mut card, "GAMETITLE", "SLOT1", &data("PlayerName, day 4", 10)).unwrap();
        let s = saves(&card).unwrap();
        assert_eq!(s.len(), 2, "the old copy is gone");
        let n = s.iter().find(|s| s.game == "GAMETITLE").unwrap();
        assert_eq!((n.title.as_str(), n.seq), ("PlayerName, day 4", 1));
        assert_eq!(free_blocks(&card).len(), BLOCKS - 1 - 2);
    }

    #[test]
    fn a_save_cut_off_before_its_commit_is_free_space() {
        let mut card = blank();
        write_save(&mut card, "PONG", "SCORES", &data("x", 2 * BLOCK)).unwrap();
        let s = saves(&card).unwrap().remove(0);
        card[entry(s.first)] = 0;
        assert_eq!(saves(&card).unwrap(), Vec::new());
        assert_eq!(free_blocks(&card).len(), BLOCKS - 1, "the orphaned second block counts as free");
    }

    #[test]
    fn a_full_card_says_so() {
        let mut card = blank();
        assert!(write_save(&mut card, "BIG", "ALL", &data("x", BLOCKS * BLOCK)).is_err());
    }
}
