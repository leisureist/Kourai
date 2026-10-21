//! Strict NDJSON line splitter.
//!
//! Feed byte chunks with [`Splitter::push`]; call [`Splitter::finish`] once after
//! the last chunk. Lines may be split across any chunk boundary. `\n` and `\r\n`
//! are accepted, blank lines are skipped, and a malformed or over-long line is
//! an [`Error`] carrying its 1-based line number — never silently dropped.

use std::fmt;

/// Default maximum byte length of one line (excluding the newline): 1 MiB.
pub const DEFAULT_MAX_LINE_BYTES: usize = 1024 * 1024;

/// Why a line was rejected.
#[derive(Debug)]
pub enum ErrorKind {
    /// The line is not valid UTF-8.
    InvalidUtf8(std::str::Utf8Error),
    /// The line is not valid JSON.
    InvalidJson(serde_json::Error),
    /// The line is longer than the configured maximum.
    LineTooLong { max: usize },
}

/// A rejected line and its 1-based line number.
#[derive(Debug)]
pub struct Error {
    pub line: u64,
    pub kind: ErrorKind,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ErrorKind::InvalidUtf8(e) => write!(f, "line {}: invalid UTF-8: {e}", self.line),
            ErrorKind::InvalidJson(e) => write!(f, "line {}: invalid JSON: {e}", self.line),
            ErrorKind::LineTooLong { max } => write!(f, "line {}: line exceeds {max} bytes", self.line),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            ErrorKind::InvalidUtf8(e) => Some(e),
            ErrorKind::InvalidJson(e) => Some(e),
            ErrorKind::LineTooLong { .. } => None,
        }
    }
}

/// Splits a chunked NDJSON byte stream into parsed values.
#[derive(Debug)]
pub struct Splitter {
    buf: Vec<u8>,
    scanned: usize,
    line: u64,
    max_line_bytes: usize,
}

impl Default for Splitter {
    fn default() -> Self {
        Self::new()
    }
}

impl Splitter {
    /// Creates a splitter with [`DEFAULT_MAX_LINE_BYTES`].
    pub fn new() -> Self {
        Self::with_max_line_bytes(DEFAULT_MAX_LINE_BYTES)
    }

    /// Creates a splitter with a custom line limit.
    ///
    /// # Panics
    /// Panics if `max_line_bytes` is 0.
    pub fn with_max_line_bytes(max_line_bytes: usize) -> Self {
        assert!(max_line_bytes > 0, "max_line_bytes must be positive");
        Self { buf: Vec::new(), scanned: 0, line: 0, max_line_bytes }
    }

    /// Adds a chunk and returns the values of every line it completed.
    ///
    /// After an error the remaining buffered input is unspecified; drop the splitter.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<serde_json::Value>, Error> {
        self.buf.extend_from_slice(chunk);
        let mut values = Vec::new();
        let mut start = 0;
        // Resume the newline search where the previous push stopped (no rescans).
        let mut from = self.scanned;
        while let Some(off) = self.buf[from..].iter().position(|&b| b == b'\n') {
            let nl = from + off;
            let raw = &self.buf[start..nl];
            self.line += 1;
            if let Some(v) = parse_line(raw, self.line, self.max_line_bytes)? {
                values.push(v);
            }
            start = nl + 1;
            from = start;
        }
        self.buf.drain(..start);
        self.scanned = self.buf.len();
        if self.buf.len() > self.max_line_bytes + 1 {
            // +1 tolerates a trailing '\r' of a CRLF whose '\n' has not arrived yet.
            return Err(Error { line: self.line + 1, kind: ErrorKind::LineTooLong { max: self.max_line_bytes } });
        }
        Ok(values)
    }

    /// Parses the last line (if it had no trailing newline) and consumes the splitter.
    pub fn finish(mut self) -> Result<Vec<serde_json::Value>, Error> {
        if self.buf.is_empty() {
            return Ok(Vec::new());
        }
        self.line += 1;
        let rest = std::mem::take(&mut self.buf);
        Ok(parse_line(&rest, self.line, self.max_line_bytes)?.into_iter().collect())
    }
}

fn parse_line(raw: &[u8], line: u64, max: usize) -> Result<Option<serde_json::Value>, Error> {
    let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
    if raw.len() > max {
        return Err(Error { line, kind: ErrorKind::LineTooLong { max } });
    }
    let text = std::str::from_utf8(raw).map_err(|e| Error { line, kind: ErrorKind::InvalidUtf8(e) })?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(text).map(Some).map_err(|e| Error { line, kind: ErrorKind::InvalidJson(e) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn complete_lines_and_buffered_partial() {
        let mut s = Splitter::new();
        assert_eq!(s.push(br#"{"a":1}
{"b""#).unwrap(), vec![json!({"a":1})]);
        assert_eq!(s.push(b":2}\n").unwrap(), vec![json!({"b":2})]);
        assert!(s.finish().unwrap().is_empty());
    }

    #[test]
    fn finish_parses_last_line_without_newline() {
        let mut s = Splitter::new();
        assert_eq!(s.push(b"1\n2").unwrap(), vec![json!(1)]);
        assert_eq!(s.finish().unwrap(), vec![json!(2)]);
    }

    #[test]
    fn crlf_and_blank_lines() {
        let mut s = Splitter::new();
        assert_eq!(s.push(b"\"x\"\r\n\r\n  \n\"y\"\n").unwrap(), vec![json!("x"), json!("y")]);
    }

    #[test]
    fn multibyte_utf8_split_byte_by_byte() {
        let mut s = Splitter::new();
        let mut out = Vec::new();
        for b in "\"台新\"\n".as_bytes() {
            out.extend(s.push(std::slice::from_ref(b)).unwrap());
        }
        out.extend(s.finish().unwrap());
        assert_eq!(out, vec![json!("台新")]);
    }

    #[test]
    fn malformed_line_reports_line_number() {
        let mut s = Splitter::new();
        let e = s.push(b"1\n\n{bad\n").unwrap_err();
        assert_eq!(e.line, 3);
        assert!(matches!(e.kind, ErrorKind::InvalidJson(_)));
    }

    #[test]
    fn malformed_last_line_fails_on_finish() {
        let mut s = Splitter::new();
        assert_eq!(s.push(b"1\n{bad").unwrap(), vec![json!(1)]);
        assert_eq!(s.finish().unwrap_err().line, 2);
    }

    #[test]
    fn invalid_utf8() {
        let mut s = Splitter::new();
        let e = s.push(&[b'"', 0xff, b'"', b'\n']).unwrap_err();
        assert!(matches!(e.kind, ErrorKind::InvalidUtf8(_)));
    }

    #[test]
    fn too_long_before_newline_arrives() {
        let mut s = Splitter::with_max_line_bytes(8);
        assert_eq!(s.push(b"\"1234\"\n").unwrap(), vec![json!("1234")]);
        let e = s.push(b"\"123456789").unwrap_err();
        assert_eq!(e.line, 2);
        assert!(matches!(e.kind, ErrorKind::LineTooLong { max: 8 }));
    }

    #[test]
    fn crlf_at_exact_limit_is_accepted() {
        let mut s = Splitter::with_max_line_bytes(3);
        assert!(s.push(b"123\r").unwrap().is_empty());
        assert_eq!(s.push(b"\n").unwrap(), vec![json!(123)]);
    }

    #[test]
    fn limit_counts_bytes() {
        let mut s = Splitter::with_max_line_bytes(6);
        assert!(s.push("\"台新\"\n".as_bytes()).is_err()); // 8 bytes
    }

    #[test]
    #[should_panic]
    fn zero_limit_panics() {
        let _ = Splitter::with_max_line_bytes(0);
    }
}
