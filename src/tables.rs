//! Table lookups over the generated data (`tables_gen.rs`).
//!
//! All tables are static, sorted, and binary-searched — the same shapes as
//! the pure-Python implementation (`bisect` over `uts46_starts`,
//! `intranges_contain` over encoded ranges), so behavior matches by
//! construction. No allocation on the lookup path.

use crate::tables_gen as g;

/// Binary search over sorted, non-overlapping `(start, end_exclusive)` ranges.
#[inline]
pub fn range_contains(table: &[(u32, u32)], cp: u32) -> bool {
    let i = table.partition_point(|&(s, _)| s <= cp);
    i > 0 && cp < table[i - 1].1
}

/// UTS-46 lookup: `(status, mapped replacement if status == M)`.
///
/// Mirrors `uts46_remap`: `i = cp` for `cp < 256` (identity prefix,
/// asserted in tests), else `bisect_right(starts, cp) - 1`.
#[inline]
pub fn uts46_lookup(cp: u32) -> (u8, Option<&'static str>) {
    let i = if cp < 256 {
        debug_assert!(g::UTS46_STARTS[cp as usize] == cp);
        cp as usize
    } else {
        match g::UTS46_STARTS.binary_search(&cp) {
            Ok(j) => j,
            // `STARTS[0] == 0`, so `j >= 1` for any `cp >= 256`.
            Err(j) => j - 1,
        }
    };
    let status = g::UTS46_STATUS[i];
    let rep = if status == b'M' {
        g::UTS46_MAP
            .binary_search_by_key(&cp, |&(c, _)| c)
            .ok()
            .map(|j| g::UTS46_MAP[j].1)
    } else {
        None
    };
    (status, rep)
}

/// Per-codepoint Unicode properties from the oracle interpreter's
/// `unicodedata` (see `tools/gen_tables.py` header for the version).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Ucd {
    pub bidi: u8,
    pub combining: u8,
    pub is_m: bool,
    pub has_name: bool,
}

#[inline]
pub fn ucd_lookup(cp: u32) -> Ucd {
    let runs = g::UCD_RUNS;
    // Runs cover `0..0x110000` contiguously starting at 0.
    let i = runs.partition_point(|&(s, _, _, _, _)| s <= cp);
    debug_assert!(i > 0);
    let (_, end, bidi, combining, flags) = runs[i - 1];
    debug_assert!(cp < end);
    Ucd {
        bidi,
        combining,
        is_m: flags & 2 != 0,
        has_name: flags & 1 != 0,
    }
}

/// Resolve a Bidi_Class name to its run-table id. Called once per label
/// (sets are resolved before the per-character loop), never per character.
pub fn bidi_id(name: &str) -> u8 {
    g::BIDI_NAMES
        .iter()
        .position(|&n| n == name)
        .expect("bidi class missing from generated table") as u8
}

/// `unicodedata.combining` equivalent: returns `None` where CPython raises
/// `ValueError` ("Unknown character in unicodedata").
#[inline]
pub fn combining_class(cp: u32) -> Option<u8> {
    let u = ucd_lookup(cp);
    if u.combining == 0 && !u.has_name {
        return None;
    }
    Some(u.combining)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_identity_prefix() {
        for i in 0..256u32 {
            assert_eq!(g::UTS46_STARTS[i as usize], i);
        }
    }

    #[test]
    fn runs_cover_all_codepoints() {
        assert_eq!(g::UCD_RUNS[0].0, 0);
        assert_eq!(g::UCD_RUNS[g::UCD_RUNS.len() - 1].1, 0x110000);
        for w in g::UCD_RUNS.windows(2) {
            assert_eq!(w[0].1, w[1].0);
        }
    }

    #[test]
    fn spot_lookups() {
        // 'A' -> M 'a'; U+00DF (deviation) kept; U+0041 covered above.
        assert_eq!(uts46_lookup(0x41), (b'M', Some("a")));
        assert_eq!(uts46_lookup(0xDF).0, b'D');
        assert_eq!(uts46_lookup(0x2E).0, b'V');
        assert!(range_contains(g::CLASS_PVALID, 0x61));
        assert!(!range_contains(g::CLASS_PVALID, 0x41));
        assert!(!range_contains(g::CLASS_PVALID, 0x5F));
        let u = ucd_lookup(0x300);
        assert_eq!((u.combining, u.is_m, u.has_name), (230, true, true));
        assert_eq!(combining_class(0x0), None);
        assert_eq!(combining_class(0x200C), Some(0));
    }
}
