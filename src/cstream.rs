// cstream.rs
// A small character stream helper that tracks byte index + (line, col).
// Not currently used by main.rs, but kept as a utility module.

use std::fs::File;
use std::io::{self, Read};

use crate::token::Span;

#[derive(Debug, Clone)]
pub struct CStream {
    src: String,
    idx: usize,   // byte offset into src
    line: usize,  // 1-based
    col: usize,   // 1-based
}

impl CStream {
    pub fn from_file(path: &str) -> io::Result<Self> {
        let mut s = String::new();
        File::open(path)?.read_to_string(&mut s)?;
        Ok(Self::from_string(s))
    }

    pub fn from_string(mut s: String) -> Self {
        // Normalize Windows newlines to keep (line, col) stable.
        if s.contains("\r\n") {
            s = s.replace("\r\n", "\n");
        }
        Self { src: s, idx: 0, line: 1, col: 1 }
    }

    #[inline]
    pub fn eof(&self) -> bool {
        self.idx >= self.src.len()
    }

    #[inline]
    pub fn peek(&self) -> Option<char> {
        self.src[self.idx..].chars().next()
    }

    #[inline]
    pub fn peek_ahead(&self, k: usize) -> Option<char> {
        self.src[self.idx..].chars().nth(k)
    }

    /// Consume one char and advance position.
    pub fn next(&mut self) -> Option<char> {
        let ch = self.peek()?;
        let len = ch.len_utf8();
        self.idx += len;

        if ch == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    /// Current position as a zero-length span.
    #[inline]
    pub fn position(&self) -> Span {
        Span { line: self.line, col: self.col, idx: self.idx, len: 0 }
    }

    #[inline]
    pub fn source(&self) -> &str {
        &self.src
    }
}
