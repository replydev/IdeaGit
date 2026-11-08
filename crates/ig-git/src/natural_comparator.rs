//! Natural sort order ("file2" < "file10"), used to compare reference names.
//!
//! Port of `platform/util/base/src/com/intellij/openapi/util/text/NaturalComparator.java` (with
//! `StringUtilRt.compare(char, char, boolean)`), which is `GitReference.REFS_NAMES_COMPARATOR`
//! (`plugins/git4idea/shared/src/git4idea/GitReference.kt`). Works on UTF-16 code units, like Java.

use std::cmp::Ordering;

/// `NaturalComparator.INSTANCE.compare`: case-insensitive natural order, then case-sensitive.
pub fn natural_compare(s1: &str, s2: &str) -> Ordering {
    let a: Vec<u16> = s1.encode_utf16().collect();
    let b: Vec<u16> = s2.encode_utf16().collect();
    natural_compare_units(&a, &b, true, false).cmp(&0)
}

/// `naturalCompare(s1, s2, ignoreCase, likeFileNames)`, returning Java's `int`.
pub fn natural_compare_with(s1: &str, s2: &str, ignore_case: bool, like_file_names: bool) -> i32 {
    let a: Vec<u16> = s1.encode_utf16().collect();
    let b: Vec<u16> = s2.encode_utf16().collect();
    natural_compare_units(&a, &b, ignore_case, like_file_names)
}

fn is_decimal_digit(c: u16) -> bool {
    (b'0' as u16..=b'9' as u16).contains(&c)
}

const SPACE: u16 = b' ' as u16;
const ZERO: u16 = b'0' as u16;

fn natural_compare_units(s1: &[u16], s2: &[u16], ignore_case: bool, like_file_names: bool) -> i32 {
    let (length1, length2) = (s1.len(), s2.len());
    let mut i = 0;
    let mut j = 0;
    while i < length1 && j < length2 {
        let ch1 = s1[i];
        let ch2 = s2[j];
        if (is_decimal_digit(ch1) || ch1 == SPACE) && (is_decimal_digit(ch2) || ch2 == SPACE) {
            let start1 = skip_char(s1, skip_char(s1, i, SPACE), ZERO);
            let start2 = skip_char(s2, skip_char(s2, j, SPACE), ZERO);

            let end1 = skip_digits(s1, start1);
            let end2 = skip_digits(s2, start2);

            // numbers with more digits are always greater than shorter numbers
            let length_diff = (end1 - start1) as i32 - (end2 - start2) as i32;
            if length_diff != 0 {
                return length_diff;
            }
            // compare numbers with equal digit count
            let number_diff = compare_char_range(s1, s2, start1, start2, end1);
            if number_diff != 0 {
                return number_diff;
            }
            // compare number length including leading spaces and zeroes
            let full_length_diff = (end1 - i) as i32 - (end2 - j) as i32;
            if full_length_diff != 0 {
                return full_length_diff;
            }
            // the numbers are the same; compare leading spaces and zeroes
            let leading_diff = compare_char_range(s1, s2, i, j, start1);
            if leading_diff != 0 {
                return leading_diff;
            }
            // `i = end1 - 1` before the loop increment; `end1 > i` here
            i = end1;
            j = end2;
            continue;
        } else if like_file_names {
            // for super natural comparison (IDEA-80435)
            if ch1 != ch2 {
                let dash = b'-' as u16;
                let underscore = b'_' as u16;
                let diff = if ch1 == dash && ch2 != underscore {
                    compare_chars(underscore, ch2, ignore_case)
                } else if ch2 == dash && ch1 != underscore {
                    compare_chars(ch1, underscore, ignore_case)
                } else {
                    compare_chars(ch1, ch2, ignore_case)
                };
                if diff != 0 {
                    return diff;
                }
            }
        } else {
            let diff = compare_chars(ch1, ch2, ignore_case);
            if diff != 0 {
                return diff;
            }
        }
        i += 1;
        j += 1;
    }
    // the end of one of the strings might not have been reached if the other string ends with a
    // number and the strings are equal until the end of that number
    if i < length1 {
        return 1;
    }
    if j < length2 {
        return -1;
    }
    if length1 != length2 {
        return length1 as i32 - length2 as i32;
    }
    // do case-sensitive compare if case-insensitive strings are equal
    if ignore_case {
        natural_compare_units(s1, s2, false, like_file_names)
    } else {
        0
    }
}

fn compare_char_range(s1: &[u16], s2: &[u16], offset1: usize, offset2: usize, end1: usize) -> i32 {
    for k in 0..end1.saturating_sub(offset1) {
        let diff = s1[offset1 + k] as i32 - s2[offset2 + k] as i32;
        if diff != 0 {
            return diff;
        }
    }
    0
}

fn compare_chars(ch1: u16, ch2: u16, ignore_case: bool) -> i32 {
    // transitivity fix, otherwise can fail when comparing strings with characters between ' ' and
    // '0' (e.g. '#')
    if ch1 == SPACE && ch2 > SPACE && ch2 < ZERO {
        return 1;
    }
    if ch2 == SPACE && ch1 > SPACE && ch1 < ZERO {
        return -1;
    }
    compare_chars_rt(ch1, ch2, ignore_case)
}

/// `StringUtilRt.compare(char, char, boolean)`.
fn compare_chars_rt(c1: u16, c2: u16, ignore_case: bool) -> i32 {
    let d = c1 as i32 - c2 as i32;
    if d == 0 || !ignore_case {
        return d;
    }
    let u1 = to_upper(c1);
    let u2 = to_upper(c2);
    let d = u1 as i32 - u2 as i32;
    if d != 0 {
        return to_lower(u1) as i32 - to_lower(u2) as i32;
    }
    d
}

/// `Character.toUpperCase(char)` for one UTF-16 unit (surrogates unchanged).
fn to_upper(c: u16) -> u16 {
    map_unit(c, |ch| ch.to_uppercase())
}

fn to_lower(c: u16) -> u16 {
    map_unit(c, |ch| ch.to_lowercase())
}

fn map_unit<I: Iterator<Item = char>>(c: u16, f: impl Fn(char) -> I) -> u16 {
    match char::from_u32(c as u32) {
        Some(ch) => {
            let mut mapped = f(ch);
            match (mapped.next(), mapped.next()) {
                // Java maps one char to one char; multi-unit mappings keep the original unit, so
                // e.g. U+0130 (İ) differs from Java. Unicode versions also differ between Java
                // and Rust. Both are negligible for ref names.
                (Some(m), None) if (m as u32) <= 0xFFFF => m as u16,
                _ => c,
            }
        }
        None => c,
    }
}

fn skip_digits(s: &[u16], mut start: usize) -> usize {
    while start < s.len() && is_decimal_digit(s[start]) {
        start += 1;
    }
    start
}

fn skip_char(s: &[u16], mut start: usize, c: u16) -> usize {
    while start < s.len() && s[start] == c {
        start += 1;
    }
    start
}

#[cfg(test)]
mod tests {
    use super::*;
    use Ordering::*;

    #[test]
    fn numbers_compare_by_value() {
        assert_eq!(natural_compare("file2", "file10"), Less);
        assert_eq!(natural_compare("v1.10", "v1.9"), Greater);
        assert_eq!(natural_compare("a01", "a1"), Greater);
        assert_eq!(natural_compare("abc", "abc"), Equal);
    }

    #[test]
    fn case_insensitive_first() {
        assert_eq!(natural_compare("Feature", "feature"), Less);
        assert_eq!(natural_compare("Zoo", "apple"), Greater);
        assert_eq!(natural_compare("feature", "release"), Less);
        assert_eq!(natural_compare("origin/aaa", "origin/feature"), Less);
        assert_eq!(natural_compare("a", "ab"), Less);
    }

    // Expected values below are derived by hand from upstream `NaturalComparator.java`.

    #[test]
    fn leading_zeros_and_spaces() {
        // a01 vs a001: equal digit count and value; full lengths 2 vs 3 -> negative
        assert_eq!(natural_compare("a01", "a001"), Less);
        assert_eq!(natural_compare("a001", "a01"), Greater);
        // a 1 vs a1: equal value; full lengths (space included) 2 vs 1 -> positive
        assert_eq!(natural_compare("a 1", "a1"), Greater);
        assert_eq!(natural_compare("a1", "a 1"), Less);
    }

    #[test]
    fn number_at_end_vs_longer_string() {
        // equal through the number, then s2 has more characters -> negative
        assert_eq!(natural_compare("a1", "a1b"), Less);
        assert_eq!(natural_compare("a1b", "a1"), Greater);
        // "10" is a longer digit run than "1" -> positive, regardless of the trailing 'b'
        assert_eq!(natural_compare("a10", "a1b"), Greater);
        assert_eq!(natural_compare("a9", "a10b"), Less);
    }

    #[test]
    fn dash_underscore_rule() {
        let cmp = |a, b| natural_compare_with(a, b, true, true).signum();
        // '-' vs '_' compares '-'(45) with '_'(95)
        assert_eq!(cmp("a-b", "a_b"), -1);
        assert_eq!(cmp("a_b", "a-b"), 1);
        // '-' vs any other char is compared as '_': '_'(95) > '0'(48) and > '+'(43)
        assert_eq!(cmp("a-b", "a0b"), 1);
        assert_eq!(cmp("a-b", "a+b"), 1);
        assert_eq!(cmp("a0b", "a-b"), -1);
        // '_' (95) vs 'a' ignoring case: upper 95 vs 65 differ, lower 95 - 97 -> negative
        assert_eq!(cmp("a-b", "aab"), -1);
        // without likeFileNames '-'(45) is compared as itself: 45 - 48 < 0
        assert_eq!(natural_compare_with("a-b", "a0b", true, false).signum(), -1);
    }

    #[test]
    fn non_ascii_and_surrogate_pairs() {
        // U+00E9 vs 'f': upper 0xC9 vs 0x46 differ; lower 0xE9 - 0x66 > 0
        assert_eq!(natural_compare("\u{e9}", "f"), Greater);
        // equal ignoring case; tie-break: 0xE9 - 0xC9 > 0
        assert_eq!(natural_compare("\u{e9}", "\u{c9}"), Greater);
        // U+1F600 = D83D DE00, U+1F601 = D83D DE01: first units equal, then DE00 < DE01
        assert_eq!(natural_compare("\u{1F600}", "\u{1F601}"), Less);
        // UTF-16 order, not code point order: D83D < FF21 although U+1F600 > U+FF21
        assert_eq!(natural_compare("\u{1F600}", "\u{FF21}"), Less);
        assert_eq!(natural_compare("\u{FF21}", "\u{1F600}"), Greater);
        // prefix: remaining units in s2 -> negative
        assert_eq!(natural_compare("\u{1F600}", "\u{1F600}a"), Less);
        assert_eq!(natural_compare("\u{1F600}a1", "\u{1F600}a1"), Equal);
    }

    #[test]
    fn final_case_sensitive_tie_break() {
        // equal ignoring case; first differing unit 'A'(65) - 'a'(97) < 0
        assert_eq!(natural_compare("Abc", "abc"), Less);
        assert_eq!(natural_compare("abc", "Abc"), Greater);
        assert_eq!(natural_compare("a1B", "a1b"), Less);
        // 'a' - 'A' > 0 at the first differing unit
        assert_eq!(natural_compare("aB1", "Ab1"), Greater);
        // the case-insensitive pass decides first: b vs C -> negative
        assert_eq!(natural_compare("Ab", "aC"), Less);
        // the case-sensitive pass is part of the natural compare, not of the *_with(false)
        assert_eq!(
            natural_compare_with("Abc", "abc", false, false).signum(),
            -1
        );
        assert_eq!(natural_compare_with("abc", "ABC", true, false).signum(), 1);
    }

    #[test]
    fn antisymmetry() {
        let samples = [
            "",
            " ",
            "  ",
            "#",
            "a",
            "A",
            "a b",
            "a#b",
            "a0b",
            "a 1",
            "a1",
            "a01",
            "a001",
            "a10",
            "a1b",
            "a-b",
            "a_b",
            "a+b",
            "aab",
            "file2",
            "file10",
            "File10",
            "v1.9",
            "v1.10",
            "origin/main",
            "origin/Main",
            "\u{e9}",
            "\u{c9}",
            "\u{1F600}",
            "\u{1F601}",
            "\u{FF21}",
            "0",
            "00",
            " 0",
            "0 ",
            "1 ",
            " 1",
        ];
        for a in samples {
            for b in samples {
                assert_eq!(
                    natural_compare(a, b),
                    natural_compare(b, a).reverse(),
                    "{a:?} vs {b:?}"
                );
                for (ic, lf) in [(true, true), (true, false), (false, true), (false, false)] {
                    assert_eq!(
                        natural_compare_with(a, b, ic, lf).signum(),
                        -natural_compare_with(b, a, ic, lf).signum(),
                        "{a:?} vs {b:?} ic={ic} lf={lf}"
                    );
                }
            }
        }
    }

    #[test]
    fn hash_space_transitivity_fix() {
        // ' ' vs '#': the fix makes ' ' greater than chars in (' ', '0'); raw ' ' - '#' would be < 0
        assert_eq!(natural_compare("a b", "a#b"), Greater);
        assert_eq!(natural_compare("a#b", "a b"), Less);
        // ' ' vs '0' goes through the number branch: equal numbers, leading ' ' (32) < '0' (48)
        assert_eq!(natural_compare("a b", "a0b"), Less);
        // '#' (35) < '0' (48) as plain chars
        assert_eq!(natural_compare("a#b", "a0b"), Less);
        // so the chain is transitive: '#' < ' ' < '0'
        let (x, y, z) = ("a#b", "a b", "a0b");
        assert_eq!(natural_compare(x, y), Less);
        assert_eq!(natural_compare(y, z), Less);
        assert_eq!(natural_compare(x, z), Less);
    }
}
