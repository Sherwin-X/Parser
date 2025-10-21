#[path = "token.rs"]
mod token;
use token::{Token, TokenType};

#[path = "cstream.rs"]
mod cstream;
use cstream::CStream;

use std::collections::HashSet;

pub struct Scanner {
    cs: CStream,
    tokens: Vec<Token>,
    keywords: HashSet<&'static str>,
}

impl Scanner {
    pub fn new(path: &str) -> Self {
        let cs = CStream::from_file(path).expect("failed to read source file");
        let keywords: HashSet<&'static str> = [
            "int","float","void","while","return","if","else","for","do","break","continue",
            "char","double","struct","union","typedef","const","static","switch","case","default"
        ].into_iter().collect();
        Self { cs, tokens: Vec::new(), keywords }
    }

    pub fn tokenize(&mut self) {
        while !self.cs.eof() {
            // whitespace
            if let Some(ws) = self.read_whitespace() {
                let (l, c) = (ws.1, ws.2);
                self.tokens.push(Token::new(ws.0, TokenType::Whitespace, l, c));
                continue;
            }
            if self.cs.eof() { break; }

            let (line, col) = self.cs.position();
            let ch = self.cs.peek().unwrap();

            // comments
            if ch == '/' && self.cs.peek_ahead(1) == Some('/') {
                let text = self.read_line_comment();
                self.tokens.push(Token::new(text, TokenType::Comment, line, col));
                continue;
            }
            if ch == '/' && self.cs.peek_ahead(1) == Some('*') {
                let text = self.read_block_comment();
                self.tokens.push(Token::new(text, TokenType::Comment, line, col));
                continue;
            }

            // string & char literal
            if ch == '"' {
                let text = self.read_string();
                self.tokens.push(Token::new(text, TokenType::StringLiteral, line, col));
                continue;
            }
            if ch == '\'' {
                let text = self.read_char();
                self.tokens.push(Token::new(text, TokenType::CharLiteral, line, col));
                continue;
            }

            // identifier / keyword
            if ch.is_ascii_alphabetic() || ch == '_' {
                let ident = self.read_identifier();
                let kind = if self.keywords.contains(ident.as_str()) { TokenType::Keyword } else { TokenType::Identifier };
                self.tokens.push(Token::new(ident, kind, line, col));
                continue;
            }

            // number
            if ch.is_ascii_digit() || (ch == '.' && self.cs.peek_ahead(1).map(|d| d.is_ascii_digit()).unwrap_or(false)) {
                let num = self.read_number();
                let kind = if num.contains(['.','e','E']) { TokenType::FloatConstant } else { TokenType::IntConstant };
                self.tokens.push(Token::new(num, kind, line, col));
                continue;
            }

            // operator or punctuation
            if Self::is_operator_start(ch) {
                let op = self.read_operator();
                self.tokens.push(Token::new(op, TokenType::Operator, line, col));
                continue;
            }

            // punctuation
            let ch = self.cs.next().unwrap();
            let text = ch.to_string();
            let kind = match ch {
                ';' | ',' | '(' | ')' | '{' | '}' | '[' | ']' => TokenType::Punctuation,
                _ => TokenType::Invalid,
            };
            self.tokens.push(Token::new(text, kind, line, col));
        }
    }

    fn read_whitespace        (&mut self) -> Option<(String, usize, usize)> {
        let (mut s, l0, c0) = (String::new(), self.cs.position().0, self.cs.position().1);
        let mut hit = false;
        while let Some(ch) = self.cs.peek() {
            if ch.is_whitespace() { s.push(ch); self.cs.next(); hit = true; } else { break; }
        }
        if hit { Some((s, l0, c0)) } else { None }
    }

    fn read_line_comment(&mut self) -> String {
        let mut s = String::new();
        self.cs.next(); self.cs.next(); // consume //
        s.push('/'); s.push('/');
        while let Some(ch) = self.cs.peek() {
            s.push(ch);
            self.cs.next();
            if ch == '\n' { break; }
        }
        s
    }

    fn read_block_comment(&mut self) -> String {
        let mut s = String::new();
        self.cs.next(); self.cs.next(); // consume /*
        s.push('/'); s.push('*');
        while let Some(ch) = self.cs.next() {
            s.push(ch);
            if ch == '*' && self.cs.peek() == Some('/') {
                s.push('/');
                self.cs.next(); // consume '/'
                break;
            }
        }
        s
    }

    fn read_string(&mut self) -> String {
        let mut s = String::new();
        let _ = self.cs.next(); // consume "
        s.push('\"');
        while let Some(ch) = self.cs.next() {
            s.push(ch);
            if ch == '\\' {
                if let Some(nxt) = self.cs.next() { s.push(nxt); }
                continue;
            }
            if ch == '\"' { break; }
            if self.cs.eof() { break; }
        }
        s
    }

    fn read_char(&mut self) -> String {
        let mut s = String::new();
        let _ = self.cs.next(); // consume '
        s.push('\'');
        while let Some(ch) = self.cs.next() {
            s.push(ch);
            if ch == '\\' {
                if let Some(nxt) = self.cs.next() { s.push(nxt); }
                continue;
            }
            if ch == '\'' { break; }
            if self.cs.eof() { break; }
        }
        s
    }

    fn read_identifier(&mut self) -> String {
        let mut s = String::new();
        while let Some(ch) = self.cs.peek() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                s.push(ch);
                self.cs.next();
            } else { break; }
        }
        s
    }

    fn read_number(&mut self) -> String {
        let mut s = String::new();
        let mut seen_dot = false;
        let mut seen_exp = false;

        while let Some(ch) = self.cs.peek() {
            if ch.is_ascii_digit() {
                s.push(ch); self.cs.next();
            } else if ch == '.' && !seen_dot && !seen_exp {
                seen_dot = true; s.push(ch); self.cs.next();
            } else if (ch == 'e' || ch == 'E') && !seen_exp {
                let sign = self.cs.peek_ahead(1);
                let d = self.cs.peek_ahead(if matches!(sign, Some('+')|Some('-')) { 2 } else { 1 });
                if d.map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    seen_exp = true;
                    s.push(ch); self.cs.next();
                    if let Some(sigch) = self.cs.peek() {
                        if sigch == '+' || sigch == '-' { s.push(sigch); self.cs.next(); }
                    }
                } else { break; }
            } else {
                break;
            }
        }
        s
    }

    fn is_operator_start(ch: char) -> bool {
        "+-*/=<>!%&|^~".contains(ch)
    }

    fn read_operator(&mut self) -> String {
        let mut s = String::new();
        let first = self.cs.next().unwrap();
        s.push(first);
        if let Some(next) = self.cs.peek() {
            let two = format!("{}{}", first, next);
            if ["==","!=",">=","<=","&&","||","<<",">>","+=","-=","*=","/=","%=","&=","|=","^="].contains(&two.as_str()) {
                self.cs.next(); s.push(next); return s;
            }
        }
        s
    }

    pub fn get_token_vec(&self) -> &Vec<Token> { &self.tokens }
}
