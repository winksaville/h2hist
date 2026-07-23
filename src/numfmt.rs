//! Number formatting helpers. `std`-only.
//!
//! - Comma-grouped integers and floats for report cells —
//!   shared presentation plumbing, not histogram logic. Kept
//!   as its own module because it is a candidate for
//!   promotion to a separate crate (iiac-perf carries the
//!   same helpers today).
//! - Shapes match iiac-perf's `fmt_commas` /
//!   `fmt_commas_f64`, so its adoption diff stays trivial.

/// Format an integer with thousands separators,
/// `12345` → `"12,345"`.
pub fn fmt_commas(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index.is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

/// Format a float with `decimals` fractional digits and
/// thousands separators on the integer part. Non-numeric or
/// beyond-u64 integer parts come back plain (no grouping).
pub fn fmt_commas_f64(value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    let (sign, body) = match text.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", text.as_str()),
    };
    let (int_part, frac_part) = match body.find('.') {
        Some(dot) => (&body[..dot], &body[dot..]),
        None => (body, ""),
    };
    match int_part.parse::<u64>() {
        Ok(int_num) => format!("{sign}{}{frac_part}", fmt_commas(int_num)),
        Err(_) => text.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Thousands separators, u64 end to end.
    #[test]
    fn commas_shapes() {
        assert_eq!(fmt_commas(0), "0");
        assert_eq!(fmt_commas(999), "999");
        assert_eq!(fmt_commas(1_000), "1,000");
        assert_eq!(fmt_commas(1_234_567), "1,234,567");
        assert_eq!(fmt_commas(u64::MAX), "18,446,744,073,709,551,615");
    }

    /// Float formatting: rounding, zero padding, sign.
    #[test]
    fn commas_f64_shapes() {
        assert_eq!(fmt_commas_f64(1234.56, 1), "1,234.6");
        // 0.95's stored double is just below 0.95, and format!
        // rounds the true stored value — so "0.9", not "1.0".
        assert_eq!(fmt_commas_f64(0.95, 1), "0.9");
        assert_eq!(fmt_commas_f64(-12.345, 2), "-12.35");
        assert_eq!(fmt_commas_f64(0.0, 0), "0");
        assert_eq!(fmt_commas_f64(8.05, 1), "8.1");
        assert_eq!(fmt_commas_f64(1_000_000.0, 3), "1,000,000.000");
        assert_eq!(fmt_commas_f64(f64::INFINITY, 1), "inf");
    }
}
