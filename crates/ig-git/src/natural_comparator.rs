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
                // Java maps one char to one char; multi-char mappings keep the original
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
}
