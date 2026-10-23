//! Fractional rank keys: sort keys that always leave room for one more key between any two.
//!
//! A key is a base-62 fraction written with the digits `0-9A-Za-z` (in ASCII order, so string order is numeric
//! order) and never ends in `0`, so there is always room before it. Moving or inserting an entry gives it a new
//! key between its neighbours and leaves every other entry alone, which keeps sync merges simple.

/// The digits, in ascending order.
const DIGITS: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const BASE: u8 = 62;

/// Why no key was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RankError {
    /// The lower key is not below the upper key.
    #[error("the keys are not in order")]
    NotOrdered,
    /// A key is empty, uses other characters or ends in `0`.
    #[error("not a rank key")]
    Invalid,
}

/// The key for the first entry of an empty list.
#[must_use]
pub fn first() -> String {
    render(&midpoint(&[], None))
}

/// A key after `key`, as short as possible.
#[must_use]
pub fn after(key: &str) -> String {
    let digits = parse(key).unwrap_or_default();
    // Bump the first digit that can be bumped and drop the rest; if every digit is the largest, go deeper.
    if let Some(at) = digits.iter().position(|&d| d < BASE - 1) {
        let mut next = digits[..=at].to_vec();
        next[at] += 1;
        return render(&next);
    }
    let mut next = digits;
    next.extend(midpoint(&[], None));
    render(&next)
}

/// A key before `key`.
#[must_use]
pub fn before(key: &str) -> String {
    let digits = parse(key).unwrap_or_else(|| midpoint(&[], None));
    render(&midpoint(&[], Some(&digits)))
}

/// A key between `low` and `high`.
///
/// # Errors
///
/// - [`RankError::Invalid`] when either is not a rank key.
/// - [`RankError::NotOrdered`] when `low` is not below `high`.
pub fn between(low: &str, high: &str) -> Result<String, RankError> {
    let (low, high) = (
        parse(low).ok_or(RankError::Invalid)?,
        parse(high).ok_or(RankError::Invalid)?,
    );
    if low >= high {
        return Err(RankError::NotOrdered);
    }
    Ok(render(&midpoint(&low, Some(&high))))
}

/// A digit string strictly between `low` (missing digits are zero) and `high` (`None` means 1.0).
fn midpoint(low: &[u8], high: Option<&[u8]>) -> Vec<u8> {
    let digit = |s: &[u8], i: usize| s.get(i).copied().unwrap_or(0);
    if let Some(high) = high {
        let common = (0..high.len())
            .take_while(|&i| digit(low, i) == high[i])
            .count();
        if common > 0 {
            let mut out = high[..common].to_vec();
            let rest_low = low.get(common..).unwrap_or(&[]);
            out.extend(midpoint(rest_low, Some(&high[common..])));
            return out;
        }
    }
    let low_digit = digit(low, 0);
    let high_digit = high.map_or(BASE, |h| digit(h, 0));
    if high_digit - low_digit > 1 {
        return vec![low_digit.midpoint(high_digit)];
    }
    // Neighbouring digits: either the high digit alone fits (when `high` has more digits after it), or go one
    // digit deeper below 1.0.
    if high.is_some_and(|h| h.len() > 1) {
        return vec![high_digit];
    }
    let mut out = vec![low_digit];
    out.extend(midpoint(low.get(1..).unwrap_or(&[]), None));
    out
}

fn parse(key: &str) -> Option<Vec<u8>> {
    let digits = key
        .bytes()
        .map(|b| {
            DIGITS
                .iter()
                .position(|&d| d == b)
                .and_then(|i| u8::try_from(i).ok())
        })
        .collect::<Option<Vec<u8>>>()?;
    (digits.last().is_some_and(|&d| d != 0)).then_some(digits)
}

fn render(digits: &[u8]) -> String {
    digits
        .iter()
        .map(|&d| char::from(DIGITS[usize::from(d)]))
        .collect()
}

#[cfg(test)]
mod tests;
