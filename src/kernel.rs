use crate::punycode;
use crate::tables::{bidi_id, combining_class, range_contains, ucd_lookup, uts46_lookup};
use crate::tables_gen as g;
use unicode_normalization::UnicodeNormalization;

/// Unit rejection: the input (or an intermediate) is invalid, or an
/// internal anomaly (overflow, missing table row) occurred. The shim falls
/// back to pure Python, which raises the exact error or returns the exact
/// value — so `Fail` can only cost speed, never correctness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fail;

#[inline]
fn to_chars(s: &str) -> Vec<u32> {
    s.chars().map(|c| c as u32).collect()
}

#[inline]
fn std3_disallowed(cp: u32) -> bool {
    // Mirrors `_std3_disallowed_re` (`[\x00-\x2c\x2f\x3a-\x40A-Z\x5b-\x60\x7b-\x7f]`);
    // only ASCII bytes can match.
    if cp >= 0x80 {
        return false;
    }
    let b = cp as u8;
    b <= 0x2C
        || b == 0x2F
        || (0x3A..=0x40).contains(&b)
        || b.is_ascii_uppercase()
        || (0x5B..=0x60).contains(&b)
        || (0x7B..=0x7F).contains(&b)
}

/// UTS-46 §4 mapping. Mirrors `uts46_remap` (`transitional` is handled by
/// the shim — deviation codepoints are always kept).
pub fn uts46_remap(domain: &str, std3_rules: bool) -> Result<String, Fail> {
    if domain.is_ascii() {
        let result = domain.to_ascii_lowercase();
        if std3_rules {
            for b in result.bytes() {
                if std3_disallowed(b as u32) {
                    return Err(Fail);
                }
            }
        }
        return Ok(result);
    }
    let chars: Vec<char> = domain.chars().collect();
    let mut output = String::new();
    for &ch in &chars {
        let cp = ch as u32;
        let (status, rep) = uts46_lookup(cp);
        match status {
            b'V' => {
                if std3_rules && std3_disallowed(cp) {
                    return Err(Fail);
                }
                output.push(ch);
            }
            b'M' => {
                let r = rep.ok_or(Fail)?;
                if std3_rules && r.chars().any(|c| std3_disallowed(c as u32)) {
                    return Err(Fail);
                }
                output.push_str(r);
            }
            b'D' => {
                if std3_rules && std3_disallowed(cp) {
                    return Err(Fail);
                }
                output.push(ch);
            }
            b'I' => {}
            _ => return Err(Fail),
        }
    }
    Ok(output.chars().nfc().collect())
}

fn is_pvalid(cp: u32) -> bool {
    range_contains(g::CLASS_PVALID, cp)
}
fn is_contextj(cp: u32) -> bool {
    range_contains(g::CLASS_CONTEXTJ, cp)
}
fn is_contexto(cp: u32) -> bool {
    range_contains(g::CLASS_CONTEXTO, cp)
}

/// `Joining_Type`, or `None` if the code point has none.
fn joining_type(cp: u32) -> Option<u8> {
    if range_contains(g::JOINING_T, cp) {
        return Some(b'T');
    }
    // Tables are disjoint (verified); check order matches `idnadata` dict.
    if range_contains(g::JOINING_C, cp) {
        return Some(b'C');
    }
    if range_contains(g::JOINING_D, cp) {
        return Some(b'D');
    }
    if range_contains(g::JOINING_L, cp) {
        return Some(b'L');
    }
    if range_contains(g::JOINING_R, cp) {
        return Some(b'R');
    }
    None
}

fn is_script(cp: u32, table: &[(u32, u32)]) -> bool {
    range_contains(table, cp)
}

fn valid_contextj(label: &[u32], pos: usize) -> Result<bool, Fail> {
    let cp = label[pos];
    if cp == 0x200C {
        if pos > 0 {
            match combining_class(label[pos - 1]) {
                None => return Err(Fail),
                Some(9) => return Ok(true),
                Some(_) => {}
            }
        }
        let mut ok = false;
        let mut i = pos;
        while i > 0 {
            i -= 1;
            match joining_type(label[i]) {
                Some(b'T') => continue,
                Some(b'L') | Some(b'D') => {
                    ok = true;
                    break;
                }
                _ => break,
            }
        }
        if !ok {
            return Ok(false);
        }
        ok = false;
        for &c in &label[pos + 1..] {
            match joining_type(c) {
                Some(b'T') => continue,
                Some(b'R') | Some(b'D') => {
                    ok = true;
                    break;
                }
                _ => break,
            }
        }
        return Ok(ok);
    }
    if cp == 0x200D {
        if pos == 0 {
            return Ok(false);
        }
        return match combining_class(label[pos - 1]) {
            None => Err(Fail),
            Some(v) => Ok(v == 9),
        };
    }
    Ok(false)
}

fn valid_contexto(label: &[u32], pos: usize) -> bool {
    let cp = label[pos];
    if cp == 0x00B7 {
        return 0 < pos
            && pos < label.len() - 1
            && label[pos - 1] == 0x006C
            && label[pos + 1] == 0x006C;
    }
    if cp == 0x0375 {
        if pos < label.len() - 1 && label.len() > 1 {
            return is_script(label[pos + 1], g::SCRIPT_GREEK);
        }
        return false;
    }
    if cp == 0x05F3 || cp == 0x05F4 {
        if pos > 0 {
            return is_script(label[pos - 1], g::SCRIPT_HEBREW);
        }
        return false;
    }
    if cp == 0x30FB {
        for &c in label {
            if c == 0x30FB {
                continue;
            }
            if is_script(c, g::SCRIPT_HIRAGANA)
                || is_script(c, g::SCRIPT_KATAKANA)
                || is_script(c, g::SCRIPT_HAN)
            {
                return true;
            }
        }
        return false;
    }
    if (0x660..=0x669).contains(&cp) {
        return !label.iter().any(|&c| (0x6F0..=0x6F9).contains(&c));
    }
    if (0x6F0..=0x6F9).contains(&cp) {
        return !label.iter().any(|&c| (0x660..=0x669).contains(&c));
    }
    false
}

/// Bidi rule state, resolved once per label (ids into `UCD_RUNS` classes).
struct Bidi {
    r: u8,
    al: u8,
    an: u8,
    l: u8,
    en: u8,
    es: u8,
    cs: u8,
    et: u8,
    on: u8,
    bn: u8,
    nsm: u8,
    unknown: u8,
}

impl Bidi {
    fn new() -> Bidi {
        Bidi {
            r: bidi_id("R"),
            al: bidi_id("AL"),
            an: bidi_id("AN"),
            l: bidi_id("L"),
            en: bidi_id("EN"),
            es: bidi_id("ES"),
            cs: bidi_id("CS"),
            et: bidi_id("ET"),
            on: bidi_id("ON"),
            bn: bidi_id("BN"),
            nsm: bidi_id("NSM"),
            unknown: bidi_id(""),
        }
    }
    fn rtl_first(&self, d: u8) -> Option<bool> {
        if d == self.r || d == self.al {
            Some(true)
        } else if d == self.l {
            Some(false)
        } else {
            None
        }
    }
    fn rtl_allowed(&self, d: u8) -> bool {
        d == self.r
            || d == self.al
            || d == self.an
            || d == self.en
            || d == self.es
            || d == self.cs
            || d == self.et
            || d == self.on
            || d == self.bn
            || d == self.nsm
    }
    fn rtl_ending(&self, d: u8) -> bool {
        d == self.r || d == self.al || d == self.en || d == self.an
    }
    fn rtl_numeric(&self, d: u8) -> bool {
        d == self.an || d == self.en
    }
    fn ltr_allowed(&self, d: u8) -> bool {
        d == self.l
            || d == self.en
            || d == self.es
            || d == self.et
            || d == self.on
            || d == self.bn
            || d == self.nsm
            || d == self.cs
    }
    fn ltr_ending(&self, d: u8) -> bool {
        d == self.l || d == self.en
    }
}

fn check_bidi(label: &[u32]) -> Result<(), Fail> {
    if label.len() > 1024 {
        return Err(Fail);
    }
    let b = Bidi::new();
    let mut rtl_label = false;
    for &cp in label {
        let d = ucd_lookup(cp).bidi;
        if d == b.unknown {
            return Err(Fail);
        }
        if d == b.r || d == b.al || d == b.an {
            rtl_label = true;
        }
    }
    if !rtl_label {
        return Ok(());
    }
    let rtl = match b.rtl_first(ucd_lookup(label[0]).bidi) {
        Some(v) => v,
        None => return Err(Fail),
    };
    let mut valid_ending = false;
    let mut ending_idx: usize = 1;
    let mut number_type: Option<u8> = None;
    for (idx0, &cp) in label.iter().enumerate() {
        let idx = idx0 + 1;
        let direction = ucd_lookup(cp).bidi;
        if rtl {
            if !b.rtl_allowed(direction) {
                return Err(Fail);
            }
            if b.rtl_ending(direction) {
                valid_ending = true;
                ending_idx = idx;
            } else if direction != b.nsm {
                valid_ending = false;
                ending_idx = idx;
            }
            if b.rtl_numeric(direction) {
                match number_type {
                    None => number_type = Some(direction),
                    Some(t) if t != direction => return Err(Fail),
                    _ => {}
                }
            }
        } else {
            if !b.ltr_allowed(direction) {
                return Err(Fail);
            }
            if b.ltr_ending(direction) {
                valid_ending = true;
                ending_idx = idx;
            } else if direction != b.nsm {
                valid_ending = false;
                ending_idx = idx;
            }
        }
    }
    if !valid_ending {
        return Err(Fail);
    }
    let _ = ending_idx;
    Ok(())
}

/// Full IDNA 2008 label validation. Mirrors `check_label`.
pub fn check_label_chars(label: &[u32]) -> Result<(), Fail> {
    if label.len() > 1024 {
        return Err(Fail);
    }
    if label.is_empty() {
        return Err(Fail);
    }
    // `valid_string_length(label, trailing_dot=True)`: chars <= 254.
    if label.len() > 254 {
        return Err(Fail);
    }
    // NFC: `unicodedata.normalize("NFC", label) != label`.
    let s: String = label
        .iter()
        .map(|&c| char::from_u32(c).ok_or(Fail))
        .collect::<Result<String, Fail>>()?;
    if s.chars().nfc().collect::<String>() != s {
        return Err(Fail);
    }
    // Hyphens: `label[2:4] == "--"`, start/end with `-`.
    if label.len() >= 4 && label[2] == 0x2D && label[3] == 0x2D {
        return Err(Fail);
    }
    if label[0] == 0x2D || label[label.len() - 1] == 0x2D {
        return Err(Fail);
    }
    // Leading combiner.
    if ucd_lookup(label[0]).is_m {
        return Err(Fail);
    }
    for (pos, &cp) in label.iter().enumerate() {
        if is_pvalid(cp) {
            continue;
        }
        if is_contextj(cp) {
            if !valid_contextj(label, pos)? {
                return Err(Fail);
            }
        } else if is_contexto(cp) {
            if !valid_contexto(label, pos) {
                return Err(Fail);
            }
        } else {
            return Err(Fail);
        }
    }
    check_bidi(label)
}

/// Mirrors `alabel`.
pub fn alabel(label: &str) -> Result<Vec<u8>, Fail> {
    if label.chars().count() > 1024 {
        return Err(Fail);
    }
    if label.is_ascii() {
        if label.is_empty() {
            return Err(Fail);
        }
        let lower = label.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("xn--") {
            if rest.is_empty() || rest.ends_with('-') {
                return Err(Fail);
            }
            let decoded = punycode::decode(rest.as_bytes())?;
            if punycode::encode(&decoded)? != rest.as_bytes() {
                return Err(Fail);
            }
            check_label_chars(&decoded)?;
            if label.len() > 63 {
                return Err(Fail);
            }
            return Ok(label.as_bytes().to_vec());
        }
        check_label_chars(&to_chars(&lower))?;
        if label.len() > 63 {
            return Err(Fail);
        }
        return Ok(label.as_bytes().to_vec());
    }
    let chars = to_chars(label);
    check_label_chars(&chars)?;
    let encoded = punycode::encode(&chars)?;
    if encoded.len() + 4 > 63 {
        return Err(Fail);
    }
    let mut out = Vec::with_capacity(encoded.len() + 4);
    out.extend_from_slice(b"xn--");
    out.extend_from_slice(&encoded);
    Ok(out)
}

/// Mirrors `ulabel` for `str` input (the shim coerces `bytes` first).
pub fn ulabel(s: &str) -> Result<String, Fail> {
    if s.chars().count() > 1024 {
        return Err(Fail);
    }
    if !s.is_ascii() {
        let chars = to_chars(s);
        check_label_chars(&chars)?;
        return Ok(s.to_string());
    }
    let lower = s.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("xn--") {
        if rest.is_empty() || rest.ends_with('-') {
            return Err(Fail);
        }
        let decoded = punycode::decode(rest.as_bytes())?;
        if punycode::encode(&decoded)? != rest.as_bytes() {
            return Err(Fail);
        }
        check_label_chars(&decoded)?;
        return decoded
            .iter()
            .map(|&c| char::from_u32(c).ok_or(Fail))
            .collect();
    }
    check_label_chars(&to_chars(&lower))?;
    Ok(lower)
}

fn split_labels(s: &str, strict: bool) -> Vec<&str> {
    if strict {
        s.split('.').collect()
    } else {
        // U+002E, U+3002, U+FF0E, U+FF61 (mirrors `_unicode_dots_re`).
        s.split(['.', '\u{3002}', '\u{FF0E}', '\u{FF61}'])
            .collect()
    }
}

/// Mirrors `encode` (input coercion, length prologue, and transitional
/// handling stay in the Python shim).
pub fn encode_address(
    s: &str,
    strict: bool,
    uts46: bool,
    std3_rules: bool,
) -> Result<Vec<u8>, Fail> {
    let mapped: String;
    let work: &str = if uts46 {
        mapped = uts46_remap(s, std3_rules)?;
        &mapped
    } else {
        s
    };
    if work.chars().count() > 254 {
        return Err(Fail);
    }
    let mut labels = split_labels(work, strict);
    if labels == [""] {
        return Err(Fail);
    }
    let mut trailing_dot = false;
    if labels.last() == Some(&"") {
        labels.pop();
        trailing_dot = true;
    }
    let mut result: Vec<Vec<u8>> = Vec::with_capacity(labels.len());
    for label in labels {
        let b = alabel(label)?;
        if b.is_empty() {
            return Err(Fail);
        }
        result.push(b);
    }
    let mut out: Vec<u8> = result.join(&b'.');
    if trailing_dot {
        out.push(b'.');
    }
    if out.len() > 253 + usize::from(trailing_dot) {
        return Err(Fail);
    }
    Ok(out)
}

/// Mirrors `decode` (input coercion and length prologue stay in the shim).
pub fn decode_address(
    s: &str,
    strict: bool,
    uts46: bool,
    std3_rules: bool,
    display: bool,
) -> Result<String, Fail> {
    let mapped: String;
    let work: &str = if uts46 {
        mapped = uts46_remap(s, std3_rules)?;
        &mapped
    } else {
        s
    };
    if work.chars().count() > 254 {
        return Err(Fail);
    }
    let mut labels = split_labels(work, strict);
    if labels == [""] {
        return Err(Fail);
    }
    let mut trailing_dot = false;
    if labels.last() == Some(&"") {
        labels.pop();
        trailing_dot = true;
    }
    let mut result: Vec<String> = Vec::with_capacity(labels.len());
    for label in labels {
        match ulabel(label) {
            Ok(u) => {
                if u.is_empty() {
                    return Err(Fail);
                }
                result.push(u);
            }
            Err(_) => {
                if display && label.chars().take(4).collect::<String>().to_lowercase() == "xn--" {
                    result.push(label.to_lowercase());
                } else {
                    return Err(Fail);
                }
            }
        }
    }
    let mut out = result.join(".");
    if trailing_dot {
        out.push('.');
    }
    Ok(out)
}
