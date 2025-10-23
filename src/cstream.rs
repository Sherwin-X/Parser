use std::fs::File;
use std::io::{self, Read};

use crate::token::Span;

pub struct CStream {
    src: String,
    idx: usize,
    line: usize,
    col: usize,
}

impl CStream {
    pub fn from_file(path: &str) -> io::Result<Self> {
        let mut s = String::new();
        File::open(path)?.read_to_string(&mut s)?;
        // 统一换行：CRLF -> LF
        s = s.replace("\r\n", "\n");
        Ok(Self { src: s, idx: 0, line: 1, col: 0 })
    }

    pub fn peek(&self) -> Option<char> {
        self.src[self.idx..].chars().next()
    }

    pub fn peek_ahead(&self, k: usize) -> Option<char> {
        self.src[self.idx..].chars().nth(k)
    }

    pub fn next(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.idx += ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
            self.col = 0;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    pub fn position(&self) -> Span {
        Span { line: self.line, col: self.col.saturating_add(1), offset: self.idx }
    }

    pub fn eof(&self) -> bool {
        self.idx >= self.src.len()
    }
}
