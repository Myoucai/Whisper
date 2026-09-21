//! Minimal backtracking regex (v1.0.5 subset), byte semantics.
//!
//! Supported: literals, `.`, `*` `+` `?` (greedy), `[...]` with ranges
//! and leading `^` negation, `^` `$` anchors, `\d \D \w \W \s \S` plus
//! escaped literals, `\b` word boundary, top-level `|`, `(...)` groups
//! (non-capturing, quantifiable), leading inline flags `(?i)` `(?m)`
//! (case-insensitive / multiline anchors; combined e.g. `(?im)`).
//! NOT supported: backreferences, lookaround, lazy quantifiers,
//! `{n,m}` counters, Unicode classes, mid-pattern inline flags.
//! Alternation picks the FIRST matching branch (leftmost), like PCRE.

/// Leading inline flags `(?i)` / `(?m)` / `(?im)` etc.
#[derive(Clone, Copy, Default, PartialEq)]
struct Flags {
    ci: bool, // case-insensitive (ASCII)
    ml: bool, // multiline: ^ after \n, $ before \n
}

fn parse_flags(pat: &[u8]) -> (Flags, &[u8]) {
    if pat.len() >= 4 && pat[0] == b'(' && pat[1] == b'?' {
        let mut i = 2;
        let mut f = Flags::default();
        while i < pat.len() && pat[i] != b')' {
            match pat[i] {
                b'i' => f.ci = true,
                b'm' => f.ml = true,
                _ => return (Flags::default(), pat), // unknown flag: not a flag group
            }
            i += 1;
        }
        if i < pat.len() && pat[i] == b')' {
            return (f, &pat[i + 1..]);
        }
    }
    (Flags::default(), pat)
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Case-insensitive literal compare (ASCII).
fn eq_ci(a: u8, b: u8) -> bool {
    a.eq_ignore_ascii_case(&b)
}

fn class_match(spec: &[u8], b: u8, ci: bool) -> bool {
    if class_match_cs(spec, b) {
        return true;
    }
    if ci {
        let lo = b.to_ascii_lowercase();
        let up = b.to_ascii_uppercase();
        if lo != b && class_match_cs(spec, lo) {
            return true;
        }
        if up != b && class_match_cs(spec, up) {
            return true;
        }
    }
    false
}

fn class_match_cs(spec: &[u8], b: u8) -> bool {
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

fn esc_match(e: u8, b: u8, ci: bool) -> bool {
    match e {
        b'd' => b.is_ascii_digit(),
        b'D' => !b.is_ascii_digit(),
        b'w' => b.is_ascii_alphanumeric() || b == b'_',
        b'W' => !(b.is_ascii_alphanumeric() || b == b'_'),
        b's' => matches!(b, b' ' | b'\t' | b'\r' | b'\n'),
        b'S' => !matches!(b, b' ' | b'\t' | b'\r' | b'\n'),
        _ => e == b || (ci && eq_ci(e, b)),
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
fn match_at(
    p: &[u8],
    pi: usize,
    pend: usize,
    t: &[u8],
    ti: usize,
    m: &[usize],
    f: Flags,
) -> Option<usize> {
    // top-level '|' split (depth/class aware), bounded by pend
    let mut depth = 0usize;
    let mut in_class = false;
    let mut i = pi;
    while i < pend {
        match p[i] {
            b'\\' => i += 1,
            b'[' => in_class = true,
            b']' if in_class => in_class = false,
            b'(' if !in_class => depth += 1,
            b')' if !in_class && depth > 0 => depth -= 1,
            b'|' if !in_class && depth == 0 => {
                if let Some(e) = match_seq(p, pi, i, t, ti, m, f) {
                    return Some(e);
                }
                return match_at(p, i + 1, pend, t, ti, m, f);
            }
            _ => {}
        }
        i += 1;
    }
    match_seq(p, pi, pend, t, ti, m, f)
}

/// Match the sequence p[start..end] at t[ti].
fn match_seq(
    p: &[u8],
    start: usize,
    end: usize,
    t: &[u8],
    ti: usize,
    m: &[usize],
    f: Flags,
) -> Option<usize> {
    if start == end {
        return Some(ti);
    }
    let (a_lo, a_hi, after_atom) = atom_bounds(p, start, m);
    // zero-width atoms (^ $ \b and empty groups): quantifiers are
    // ignored, the atom just has to match once here
    let zero_width = p[a_lo] == b'^'
        || p[a_lo] == b'$'
        || (p[a_lo] == b'\\' && a_hi == a_lo + 2 && matches!(p[a_lo + 1], b'b'))
        || (a_hi <= a_lo + 2 && p[a_lo] == b'(');
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
        match atom_once(p, a_lo, a_hi, t, cur, m, f) {
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
        if let Some(r) = match_seq(p, next, end, t, ends[k], m, f) {
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
fn atom_once(
    p: &[u8],
    lo: usize,
    hi: usize,
    t: &[u8],
    ti: usize,
    m: &[usize],
    f: Flags,
) -> Option<usize> {
    if lo >= hi {
        return Some(ti);
    }
    match p[lo] {
        b'.' => (ti < t.len()).then_some(ti + 1),
        b'[' => {
            if ti < t.len() && class_match(&p[lo + 1..hi - 1], t[ti], f.ci) {
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
            match_at(p, lo + 1, hi - 1, t, ti, m, f)
        }
        b'^' => {
            let ok = ti == 0 || (f.ml && t[ti - 1] == b'\n');
            ok.then_some(ti)
        }
        b'$' => {
            let ok = ti == t.len() || (f.ml && t[ti] == b'\n');
            ok.then_some(ti)
        }
        b'\\' => match p[lo + 1] {
            b'b' => {
                // word boundary: \w on exactly one side
                let before = ti > 0 && is_word(t[ti - 1]);
                let after = ti < t.len() && is_word(t[ti]);
                (before != after).then_some(ti)
            }
            c => {
                if ti < t.len() && esc_match(c, t[ti], f.ci) {
                    Some(ti + 1)
                } else {
                    None
                }
            }
        },
        c => {
            if ti < t.len() && (t[ti] == c || (f.ci && eq_ci(t[ti], c))) {
                Some(ti + 1)
            } else {
                None
            }
        }
    }
}

/// Full-anchored match: the pattern must consume the entire text.
pub fn str_match(text: &[u8], pat: &[u8]) -> bool {
    let (f, pat) = parse_flags(pat);
    let m = precompute(pat);
    matches!(match_at(pat, 0, pat.len(), text, 0, &m, f), Some(e) if e == text.len())
}

/// Unanchored search: smallest index where the pattern matches a prefix
/// of the remaining text; -1 if none.
pub fn str_find(text: &[u8], pat: &[u8]) -> i64 {
    let (f, pat) = parse_flags(pat);
    let m = precompute(pat);
    for start in 0..=text.len() {
        if match_at(pat, 0, pat.len(), text, start, &m, f).is_some() {
            return start as i64;
        }
    }
    -1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_flags_and_word_boundary() {
        // Python-re-verified ground truth (2026-08-28, see lexicon L0):
        // each case checked against `re.search` with identical semantics.
        // (?i) case-insensitive search
        assert_eq!(
            str_find(b"SQLITE: Unrecognized Token 'x'", b"(?i)unrecognized token"),
            8
        );
        assert_eq!(str_find(b"all fine here", b"(?i)unrecognized token"), -1);
        assert_eq!(str_find(b"near \"1\": syntax error", b"(?i)near \""), 0);
        assert_eq!(
            str_find(
                b"DB error: Column hello not found!",
                b"(?i)column .* not found"
            ),
            10
        );
        // \b word boundary + alternation + \d
        assert_eq!(
            str_find(
                b"driver: postgres-42 wrong",
                b"(?i)\\b(ora|postgres|psycopg2|mysql|mariadb|sqlite)-\\d"
            ),
            8
        );
        assert_eq!(
            str_find(b"out\nsh: 1: X: not found", b"(?i)\\bsh: .*: not found"),
            4
        );
        // \b must not match inside a word
        assert_eq!(
            str_find(b"fish: 1: not found", b"(?i)\\bsh: .*: not found"),
            -1
        );
        // (?m) multiline anchors
        assert_eq!(
            str_find(b"root:x:0:0\n/bin/sh:\n/bin/dash:", b"(?m)^/bin/(sh|bash):"),
            11
        );
        assert_eq!(str_find(b"x /bin/sh: y", b"(?m)^/bin/(sh|bash):"), -1);
        assert_eq!(str_find(b"a\n/dev/null", b"(?m)^/dev/"), 2);
        // non-multiline ^ stays line-start-only
        assert_eq!(str_find(b"a\n/dev/null", b"^/dev/"), -1);
        // \b at end + \s+
        assert_eq!(
            str_find(
                b"Found. Redirecting to http://evil",
                b"(?i)\\bfound\\.\\s+redirecting\\s+to\\b"
            ),
            0
        );
        assert_eq!(
            str_find(
                b"found.. redirecting too",
                b"(?i)\\bfound\\.\\s+redirecting\\s+to\\b"
            ),
            -1
        );
        // passwd-shaped class pattern
        assert_eq!(
            str_find(b"root:x:0:0:root:/root:", b"root:[^:]*:[0-9]+:[0-9]+:"),
            0
        );
        // (?im) combined
        assert_eq!(str_find(b"X\nSQL Error here", b"(?im)^sql error"), 2);
        // unknown flag group is not flags: it falls back to group
        // semantics — `(?q)x` = group matching literal "?q" then "x"
        assert_eq!(str_find(b"a?qx", b"(?q)x"), 1);
    }

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
