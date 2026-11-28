//! XML serialization through quick-xml's UTF-8 writer.

use quick_xml::{se, SeError};
use serde::ser::Serialize;

/// Serialize to an `io::Write`r, including any final flush errors.
pub fn to_writer<W: std::io::Write, S: Serialize>(mut writer: W, value: &S) -> Result<(), SeError> {
    se::to_utf8_io_writer(&mut writer, value)?;
    writer.flush()?;
    Ok(())
}

/// Serialize to a `fmt::Write`r.
pub fn to_fmt_writer<W: std::fmt::Write, S: Serialize>(writer: W, value: &S) -> Result<(), SeError> {
    se::to_writer(writer, value).map(|_| ())
}

/// Serialize to a `String`.
pub fn to_string<S: Serialize>(value: &S) -> Result<String, SeError> {
    se::to_string(value)
}

/// Serialize to a `Vec<u8>`.
pub fn to_bytes<S: Serialize>(value: &S) -> Result<Vec<u8>, SeError> {
    let mut bytes = Vec::new();
    to_writer(&mut bytes, value)?;
    Ok(bytes)
}
