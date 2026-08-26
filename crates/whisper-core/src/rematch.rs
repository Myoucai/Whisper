//! Minimal backtracking regex (v1.0.5 subset), byte semantics.
//!
//! Supported: literals, `.`, `*` `+` `?` (greedy), `[...]` with ranges
//! and leading `^` negation, `^` `$` anchors, `\d \D \w \W \s \S` plus
//! escaped literals, top-level `|`, `(...)` groups (non-capturing,
//! quantifiable). NOT supported: backreferences, lookaround, lazy
//! quantifiers, `{n,m}` counters, Unicode classes. Alternation picks
//! the FIRST matching branch (leftmost), like PCRE.

fn class_match(spec: &[u8], b: u8) -> bool {
    let (neg, spec) = if !spec.is_empty() && spec[0] == b'^' {
        (true, &spec[1..])
    } else {
        (false, spec)
    };
    let mut i = 0;
    let mut hit = false;
    while i < spec.len() {
        if i + 2 < spec.len() && spec[i + 1] == b'-' {
            if spec[i] <= b && b <= spec[i + 2] {
                hit = true;
            }
            i += 3;
        } else {
            if spec[i] == b {
                hit = true;
            }
            i += 1;
        }
    }
    hit != neg
}

fn esc_match(e: u8, b: u8) -> bool {
    match e {
        b'd' => b.is_ascii_digit(),
        b'D' => !b.is_ascii_digit(),
        b'w' => b.is_ascii_alphanumeric() || b == b'_',
        b'W' => !(b.is_ascii_alphanumeric() || b == b'_'),
        b's' => matches!(b, b' ' | b'\t' | b'\r' | b'\n'),
        b'S' => !matches!(b, b' ' | b'\t' | b'\r' | b'\n'),
        _ => e == b,
    }
}

/// Precompute opener->closer mapping for `[` and `(` (0 elsewhere).
fn precompute(p: &[u8]) -> Vec<usize> {
    let mut m = vec![0usize; p.len()];
    let mut stack = Vec::new();
    let mut in_class = false;
    let mut i = 0;
    while i < p.len() {
        match p[i] {
            b'\\' => i += 1,
            b'[' if !in_class => {
                in_class = true;
                stack.push(i);
            }
            b']' if in_class => {
                in_class = false;
                if let Some(o) = stack.pop() {
                    m[o] = i;
                }
            }
            b'(' if !in_class => stack.push(i),
            b')' if !in_class => {
                if let Some(o) = stack.pop() {
                    m[o] = i;
                }
            }
            _ => {}
        }
        i += 1;
    }
    m
}

/// Match pattern p[pi..pend] against t starting at ti. Returns the end
/// offset in t of the leftmost-first (greedy) parse on success.
fn match_at(p: &[u8], pi: usize, pend: usize, t: &[u8], ti: usize, m: &[usize]) -> Option<usize> {
    // top-level '|' split (depth/class aware), bounded by pend
    let mut depth = 0usize;
    let mut in_class = false;
    let mut i = pi;
    while i < pend {
        match p[i] {
            b'\\' => i += 1,
            b'[' => in_class = true,
            b']' => in_class = false,
            b'(' if !in_class => depth += 1,
            b')' if !in_class => depth -= 1,
            b'|' if !in_class && depth == 0 => {
                if let Some(e) = match_seq(p, pi, i, t, ti, m) {
                    return Some(e);
                }
                return match_at(p, i + 1, pend, t, ti, m);
            }
            _ => {}
        }
        i += 1;
    }
    match_seq(p, pi, pend, t, ti, m)
}

/// Match the sequence p[start..end] at t[ti].
fn match_seq(
    p: &[u8],
    start: usize,
    end: usize,
    t: &[u8],
    ti: usize,
    m: &[usize],
) -> Option<usize> {
    if start == end {
        return Some(ti);
    }
    let (a_lo, a_hi, after_atom) = atom_bounds(p, start, m);
    // zero-width atoms (^ $ and empty groups): quantifiers are ignored,
    // the atom just has to match once here
    let zero_width = p[a_lo] == b'^' || p[a_lo] == b'$' || a_hi <= a_lo + 2 && p[a_lo] == b'(';
    let next = after_atom;
    let (qmin, qmax, next) = if zero_width {
        (1, 1, next)
    } else if next < end {
        match p[next] {
            b'*' => (0, usize::MAX, next + 1),
            b'+' => (1, usize::MAX, next + 1),
            b'?' => (0, 1, next + 1),
            _ => (1, 1, next),
        }
    } else {
        (1, 1, next)
    };

    // greedy collection of atom end positions (ends[k] = pos after k atoms)
    let mut ends: Vec<usize> = vec![ti];
    let mut cur = ti;
    loop {
        if ends.len() as u64 > qmax as u64 {
            break;
        }
        match atom_once(p, a_lo, a_hi, t, cur, m) {
            Some(n) => {
                if zero_width {
                    ends.push(n); // single zero-width success, stop
                    break;
                }
                if n > cur {
                    cur = n;
                    ends.push(n);
                } else {
                    break; // pathological empty repetition: stop
                }
            }
            None => break,
        }
    }
    if (ends.len() as u64) < qmin as u64 + 1 {
        return None;
    }
    // back off from the longest parse
    let mut k = ends.len();
    while k > qmin {
        k -= 1;
        if let Some(r) = match_seq(p, next, end, t, ends[k], m) {
            return Some(r);
        }
    }
    None
}

/// (atom_lo, atom_hi, index just after the atom unit)
fn atom_bounds(p: &[u8], start: usize, m: &[usize]) -> (usize, usize, usize) {
    match p[start] {
        b'(' => (start, m[start] + 1, m[start] + 1),
        b'[' => (start, m[start] + 1, m[start] + 1),
        b'\\' => (start, start + 2, start + 2),
        _ => (start, start + 1, start + 1),
    }
}

/// Match one occurrence of atom p[lo..hi] at t[ti].
fn atom_once(p: &[u8], lo: usize, hi: usize, t: &[u8], ti: usize, m: &[usize]) -> Option<usize> {
    if lo >= hi {
        return Some(ti);
    }
    match p[lo] {
        b'.' => (ti < t.len()).then_some(ti + 1),
        b'[' => {
            if ti < t.len() && class_match(&p[lo + 1..hi - 1], t[ti]) {
                Some(ti + 1)
            } else {
                None
            }
        }
        b'(' => {
            if hi - lo < 2 {
                return Some(ti); // "()" empty group
            }
            // bounded recursion: the group body is p[lo+1..hi-1]
            match_at(p, lo + 1, hi - 1, t, ti, m)
        }
        b'^' => (ti == 0).then_some(ti),
        b'$' => (ti == t.len()).then_some(ti),
        b'\\' => {
            if ti < t.len() && esc_match(p[lo + 1], t[ti]) {
                Some(ti + 1)
            } else {
                None
            }
        }
        c => {
            if ti < t.len() && t[ti] == c {
                Some(ti + 1)
            } else {
                None
            }
        }
    }
}

/// Full-anchored match: the pattern must consume the entire text.
pub fn str_match(text: &[u8], pat: &[u8]) -> bool {
    let m = precompute(pat);
    matches!(match_at(pat, 0, pat.len(), text, 0, &m), Some(e) if e == text.len())
}

/// Unanchored search: smallest index where the pattern matches a prefix
/// of the remaining text; -1 if none.
pub fn str_find(text: &[u8], pat: &[u8]) -> i64 {
    let m = precompute(pat);
    for start in 0..=text.len() {
        if match_at(pat, 0, pat.len(), text, start, &m).is_some() {
            return start as i64;
        }
    }
    -1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basics() {
        assert!(str_match(b"hello", b"h.llo"));
        assert!(!str_match(b"hello", b"hxllo"));
        assert!(str_match(b"aaa", b"a*"));
        assert!(str_match(b"", b"a*"));
        assert!(str_match(b"abc", b"a?bc"));
        assert!(str_match(b"bc", b"a?bc"));
    }

    #[test]
    fn classes_escapes() {
        assert!(str_match(b"abc123def", b"[a-z]+[0-9]+[a-z]+"));
        assert!(str_match(b"z", b"[^a-y]"));
        assert!(!str_match(b"a", b"[^a-y]"));
        assert_eq!(str_find(b"abc123", br"\d+"), 3);
        assert_eq!(str_find(b"2026-08-26", br"\d\d\d\d"), 0);
        assert!(str_match(b"a b", br"\w\s\w"));
    }

    #[test]
    fn alternation_groups() {
        assert!(str_match(b"foo", b"foo|baz"));
        assert!(str_match(b"baz", b"foo|baz"));
        assert!(!str_match(b"foo bar", b"foo|baz")); // full-anchor
        assert!(str_match(b"abab", b"(ab)+"));
        assert!(!str_match(b"aba", b"(ab)+"));
        assert!(str_match(b"cab", b"c(a|b)+"));
    }

    #[test]
    fn anchors() {
        assert!(str_match(b"hello", b"^hello$"));
        assert!(!str_match(b"say hello", b"^hello$"));
        assert_eq!(str_find(b"say hello", b"^say"), 0);
        assert_eq!(str_find(b"say hello", b"^hello"), -1);
    }

    #[test]
    fn backtracking() {
        assert!(str_match(b"aaab", b"a*ab")); // greedy must back off
        assert!(str_match(b"<tag>", b"<.+>"));
        assert!(!str_match(b"<tag", b"<.+>"));
    }
}
