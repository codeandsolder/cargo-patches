//! Shared reading-order numerical and character helpers.

/// Median as numpy computes it on float32 data.
pub(crate) fn median_f32(values: &mut [f32]) -> f32 {
    values.sort_by(f32::total_cmp);
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

/// Python's `str.isdigit` for one character, closely enough for features:
/// decimal and superscript-style digits, not fractions or numeral letters.
pub(crate) fn is_digit(c: char) -> bool {
    c.is_numeric()
        && !matches!(c,
            '\u{BC}'..='\u{BE}' | '\u{2150}'..='\u{218B}' | '\u{09F4}'..='\u{09F9}'
            | '\u{3007}' | '\u{3021}'..='\u{3029}' | '\u{3038}'..='\u{303A}'
            | '\u{4E00}'..='\u{9FFF}')
}

pub(crate) fn median_f64(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}
