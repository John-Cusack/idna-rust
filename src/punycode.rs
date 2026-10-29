//! RFC 3492 Punycode, ported exactly from CPython's
//! `Lib/encodings/punycode.py` (the codec `idna` calls via
//! `str.encode("punycode")` / `bytes.decode("punycode")`).
//!
//! Arithmetic uses checked `u64` ops: any overflow reports `Fail` and the
//! Python shim falls back to the pure-Python codec, which computes the
//! exact result with bigints. False rejects cost speed; wrong accepts
//! would cost correctness — so overflow can only reject, never wrap.

use crate::kernel::Fail;

const DIGITS: &[u8; 36] = b"abcdefghijklmnopqrstuvwxyz0123456789";

#[inline]
fn t(j: u64, bias: u64) -> u64 {
    // Punycode parameters: tmin = 1, tmax = 26, base = 36.
    let res = 36 * (j + 1);
    let res = res.saturating_sub(bias);
    if res < 1 {
        return 1;
    }
    if res > 26 {
        return 26;
    }
    res
}

fn adapt(mut delta: u64, first: bool, numchars: u64) -> Result<u64, Fail> {
    if first {
        delta /= 700;
    } else {
        delta /= 2;
    }
    delta = delta.checked_add(delta / numchars).ok_or(Fail)?;
    // ((base - tmin) * tmax) // 2 == 455
    let mut divisions: u64 = 0;
    while delta > 455 {
        delta /= 35; // base - tmin
        divisions = divisions.checked_add(36).ok_or(Fail)?;
    }
    let bias = divisions
        .checked_add(36 * delta / (delta + 38))
        .ok_or(Fail)?;
    Ok(bias)
}

fn generate_generalized_integer(mut n: u64, bias: u64) -> Result<Vec<u8>, Fail> {
    let mut result = Vec::new();
    let mut j: u64 = 0;
    loop {
        let t = t(j, bias);
        if n < t {
            result.push(DIGITS[n as usize]);
            return Ok(result);
        }
        let q = (n - t) % (36 - t);
        result.push(DIGITS[(t + q) as usize]);
        n = (n - t) / (36 - t);
        j = j.checked_add(1).ok_or(Fail)?;
    }
}
/// `punycode_encode`: basic code points in order, `-` delimiter, then the
/// generalized integers. Input is the label as code points.
pub fn encode(text: &[u32]) -> Result<Vec<u8>, Fail> {
    // segregate: basics in order; extended = sorted unique non-basics.
    let mut base: Vec<u8> = Vec::new();
    let mut extended: Vec<u32> = Vec::new();
    for &cp in text {
        if cp < 128 {
            base.push(cp as u8);
        } else {
            extended.push(cp);
        }
    }
    extended.sort_unstable();
    extended.dedup();
    // insertion_unsort: deltas as i128 (CPython uses unbounded ints;
    // any negativity/overflow rejects into the Python fallback).
    let mut deltas: Vec<u64> = Vec::new();
    let mut oldchar: u64 = 0x80;
    let mut oldindex: i64 = -1;
    for &c in &extended {
        let mut curlen: i128 = 0;
        for &x in text {
            if x < c {
                curlen += 1;
            }
        }
        let mut delta: i128 =
            (curlen + 1) * (c as i128 - oldchar as i128);
        let mut index: i64 = -1;
        let mut pos: i64 = -1;
        loop {
            // selective_find(text, c, index, pos)
            let mut p = pos + 1;
            let mut idx = index;
            let mut hit: Option<(i64, i64)> = None;
            while (p as usize) < text.len() {
                let x = text[p as usize];
                if x == c {
                    hit = Some((idx + 1, p));
                    break;
                }
                if x < c {
                    idx += 1;
                }
                p += 1;
            }
            match hit {
                None => break,
                Some((ni, np)) => {
                    index = ni;
                    pos = np;
                    delta += (index - oldindex) as i128;
                    if delta < 1 {
                        return Err(Fail);
                    }
                    deltas.push((delta - 1) as u64);
                    oldindex = index;
                    delta = 0;
                }
            }
        }
        oldchar = c as u64;
    }
    // generate_integers
    let mut out: Vec<u8> = Vec::new();
    let mut bias: u64 = 72;
    for (points, &d) in deltas.iter().enumerate() {
        out.extend(generate_generalized_integer(d, bias)?);
        bias = adapt(d, points == 0, base.len() as u64 + points as u64 + 1)?;
    }
    if base.is_empty() {
        Ok(out)
    } else {
        let mut res = base;
        res.push(b'-');
        res.extend(out);
        Ok(res)
    }
}

fn decode_generalized_number(
    extended: &[u8],
    mut extpos: usize,
    bias: u64,
) -> Result<(usize, u64), Fail> {
    let mut result: u64 = 0;
    let mut w: u64 = 1;
    let mut j: u64 = 0;
    loop {
        let &b = extended.get(extpos).ok_or(Fail)?; // "incomplete punicode string"
        extpos += 1;
        let digit: u64 = if (0x41..=0x5A).contains(&b) {
            (b - 0x41) as u64
        } else if (0x30..=0x39).contains(&b) {
            (b as u64).checked_sub(22).ok_or(Fail)? // 0x30 - 26
        } else {
            return Err(Fail); // strict: invalid extended code point
        };
        let t = t(j, bias);
        result = result
            .checked_add(digit.checked_mul(w).ok_or(Fail)?)
            .ok_or(Fail)?;
        if digit < t {
            return Ok((extpos, result));
        }
        w = w.checked_mul(36 - t).ok_or(Fail)?;
        j = j.checked_add(1).ok_or(Fail)?;
    }
}

/// `punycode_decode` (strict): ASCII bytes in, code points out.
pub fn decode(input: &[u8]) -> Result<Vec<u32>, Fail> {
    if !input.is_ascii() {
        return Err(Fail);
    }
    let pos = input.iter().rposition(|&b| b == b'-');
    let (base_b, ext_b) = match pos {
        None => (&[][..], input),
        Some(p) => (&input[..p], &input[p + 1..]),
    };
    let mut base: Vec<u32> = base_b.iter().map(|&b| b as u32).collect();
    let upper: Vec<u8> = ext_b.to_ascii_uppercase();
    // insertion_sort
    let mut char: u64 = 0x80;
    let mut pos_: i64 = -1;
    let mut bias: u64 = 72;
    let mut extpos: usize = 0;
    let mut first = true;
    while extpos < upper.len() {
        let (newpos, delta) = decode_generalized_number(&upper, extpos, bias)?;
        pos_ = pos_.checked_add(delta as i64 + 1).ok_or(Fail)?;
        char = char
            .checked_add(pos_ as u64 / (base.len() as u64 + 1))
            .ok_or(Fail)?;
        if char > 0x10FFFF {
            return Err(Fail);
        }
        pos_ %= base.len() as i64 + 1;
        base.insert(pos_ as usize, char as u32);
        bias = adapt(delta, first, base.len() as u64)?;
        first = false;
        extpos = newpos;
    }
    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_vectors() {
        // (unicode, punycode-bytes) — `xn--` prefix excluded.
        let cases: &[(&[u32], &[u8])] = &[
            (&[0x6D, 0xFC, 0x6E, 0x63, 0x68, 0x65, 0x6E], b"mnchen-3ya"),
            (&[0x65], b"e-"),
            (&[0xE9, 0x63, 0x6F, 0x6C, 0x65], b"cole-9oa"),
        ];
        for (uni, puny) in cases {
            assert_eq!(encode(uni).unwrap(), **puny);
            assert_eq!(decode(puny).unwrap(), **uni);
        }
        // pure-basic label: delimiter form.
        assert_eq!(encode(&[0x61, 0x62, 0x63]).unwrap(), b"abc-");
    }

    #[test]
    fn decode_rejects() {
        assert!(decode(b"\xff").is_err());
        assert!(decode(b"a!").is_err());
    }
}
