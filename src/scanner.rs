pub use crate::token::{Token, TokenType, Span};

#[path = "token.rs"]
pub mod token;

#[path = "cstream.rs"]
mod cstream;
use cstream::CStream;

use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub message: String,
    pub start: Span,
}

pub struct Scanner {
    cs: CStream,
    tokens: Vec<Token>,
    keywords: HashSet<&'static str>,
    diagnostics: Vec<Diagnostic>,
}

impl Scanner {
    pub fn new(path: &str) -> Self {
        let cs = CStream::from_file(path).expect("failed to read source file");
        let keywords: HashSet<&'static str> = [
            "int","float","void","while","return","if","else","for","do","break","continue",
            "char","double","struct","union","typedef","const","static","switch","case","default"
        ].into_iter().collect();
        Self { cs, tokens: Vec::new(), keywords, diagnostics: Vec::new() }
    }

    pub fn with_keywords(path: &str, kws: &[&'static str]) -> Self {
        let cs = CStream::from_file(path).expect("failed to read source file");
        let keywords: HashSet<&'static str> = kws.iter().copied().collect();
        Self { cs, tokens: Vec::new(), keywords, diagnostics: Vec::new() }
    }

    pub fn tokenize(&mut self) {
        while !self.cs.eof() {
            let tok = self.next_token_internal();
            if let Some(t) = tok {
                self.tokens.push(t);
            } else {
                break;
            }
        }
    }

    fn next_token_internal(&mut self) -> Option<Token> {
        if self.cs.eof() { return None; }

        // 预处理行：列 1 且以 # 开始
        if self.cs.peek() == Some('#') && (self.cs.position().col == 1) {
            let start = self.cs.position();
            let mut text = String::new();
            text.push(self.cs.next().unwrap());
            while let Some(ch) = self.cs.peek() {
                text.push(ch);
                self.cs.next();
                if ch == '\n' { break; }
            }
            let end = self.cs.position();
            return Some(Token::new(text, TokenType::Preprocessor, start, end));
        }

        // 空白
        if let Some(tok) = self.read_whitespace() {
            return Some(tok);
        }
        if self.cs.eof() { return None; }

        let start = self.cs.position();
        let ch = self.cs.peek().unwrap();

        // 注释
        if ch == '/' && self.cs.peek_ahead(1) == Some('/') {
            return Some(self.read_line_comment());
        }
        if ch == '/' && self.cs.peek_ahead(1) == Some('*') {
            return Some(self.read_block_comment());
        }

        // 字符串 / 字符
        if ch == '"' {
            return Some(self.read_string());
        }
        if ch == '\'' {
            return Some(self.read_char());
        }

        // 标识符 / 关键字
        if ch.is_ascii_alphabetic() || ch == '_' {
            let ident = self.read_identifier();
            let end = self.cs.position();
            let kind = if self.keywords.contains(ident.as_str()) { TokenType::Keyword } else { TokenType::Identifier };
            return Some(Token::new(ident, kind, start, end));
        }

        // 数字
        if ch.is_ascii_digit() || (ch == '.' && self.cs.peek_ahead(1).map(|d| d.is_ascii_digit()).unwrap_or(false)) {
            let (num, is_float) = self.read_number();
            let end = self.cs.position();
            let kind = if is_float { TokenType::FloatConstant } else { TokenType::IntConstant };
            return Some(Token::new(num, kind, start, end));
        }

        // 运算符
        if Self::is_operator_start(ch) {
            let op = self.read_operator();
            let end = self.cs.position();
            return Some(Token::new(op, TokenType::Operator, start, end));
        }

        // 标点或非法
        let ch = self.cs.next().unwrap();
        let end = self.cs.position();
        let text = ch.to_string();
        let kind = match ch {
            ';' | ',' | '(' | ')' | '{' | '}' | '[' | ']' => TokenType::Punctuation,
            _ => {
                self.diagnostics.push(Diagnostic{ message: format!("Unexpected character '{}'", ch), start });
                TokenType::Invalid
            },
        };
        Some(Token::new(text, kind, start, end))
    }

    fn read_whitespace(&mut self) -> Option<Token> {
        let start = self.cs.position();
        let mut s = String::new();
        let mut hit = false;
        while let Some(ch) = self.cs.peek() {
            if ch.is_whitespace() { s.push(ch); self.cs.next(); hit = true; } else { break; }
        }
        if hit {
            let end = self.cs.position();
            Some(Token::new(s, TokenType::Whitespace, start, end))
        } else {
            None
        }
    }

    fn read_line_comment(&mut self) -> Token {
        let start = self.cs.position();
        let mut s = String::new();
        self.cs.next(); self.cs.next(); // //
        s.push('/'); s.push('/');
        while let Some(ch) = self.cs.peek() {
            s.push(ch);
            self.cs.next();
            if ch == '\n' { break; }
        }
        let end = self.cs.position();
        Token::new(s, TokenType::Comment, start, end)
    }

    fn read_block_comment(&mut self) -> Token {
        let start = self.cs.position();
        let mut s = String::new();
        self.cs.next(); self.cs.next(); // /*
        s.push('/'); s.push('*');
        let mut terminated = false;
        while let Some(ch) = self.cs.next() {
            s.push(ch);
            if ch == '*' && self.cs.peek() == Some('/') {
                s.push('/');
                self.cs.next(); // consume '/'
                terminated = true;
                break;
            }
        }
        let end = self.cs.position();
        if !terminated {
            self.diagnostics.push(Diagnostic{ message:"Unterminated block comment".into(), start });
        }
        Token::new(s, TokenType::Comment, start, end)
    }

    fn read_string(&mut self) -> Token {
        let start = self.cs.position();
        let mut s = String::new();
        let _ = self.cs.next(); // "
        s.push('\"');
        let mut ok = false;
        while let Some(ch) = self.cs.next() {
            s.push(ch);
            if ch == '\\' {
                if let Some(nxt) = self.cs.next() { s.push(nxt); }
                continue;
            }
            if ch == '\"' { ok = true; break; }
        }
        let end = self.cs.position();
        if !ok {
            self.diagnostics.push(Diagnostic{ message:"Unterminated string literal".into(), start });
            return Token::new(s, TokenType::Invalid, start, end);
        }
        Token::new(s, TokenType::StringLiteral, start, end)
    }

    fn read_char(&mut self) -> Token {
        let start = self.cs.position();
        let mut s = String::new();
        let _ = self.cs.next(); // '
        s.push('\'');
        let mut ok = false;
        while let Some(ch) = self.cs.next() {
            s.push(ch);
            if ch == '\\' {
                if let Some(nxt) = self.cs.next() { s.push(nxt); }
                continue;
            }
            if ch == '\'' { ok = true; break; }
        }
        let end = self.cs.position();
        if !ok {
            self.diagnostics.push(Diagnostic{ message:"Unterminated char literal".into(), start });
            return Token::new(s, TokenType::Invalid, start, end);
        }
        Token::new(s, TokenType::CharLiteral, start, end)
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

    fn read_number(&mut self) -> (String, bool) {
        let mut s = String::new();
        let mut is_float = false;

        if self.cs.peek() == Some('0') {
            let a1 = self.cs.peek_ahead(1);
            if matches!(a1, Some('x') | Some('X')) {
                s.push(self.cs.next().unwrap()); s.push(self.cs.next().unwrap());
                while let Some(ch) = self.cs.peek() {
                    if ch.is_ascii_hexdigit() || ch == '_' { s.push(ch); self.cs.next(); } else { break; }
                }
                s.push_str(&self.read_num_suffix());
                return (s, false);
            } else if matches!(a1, Some('b') | Some('B')) {
                s.push(self.cs.next().unwrap()); s.push(self.cs.next().unwrap());
                while let Some(ch) = self.cs.peek() {
                    if ch == '0' || ch == '1' || ch == '_' { s.push(ch); self.cs.next(); } else { break; }
                }
                s.push_str(&self.read_num_suffix());
                return (s, false);
            } else if matches!(a1, Some('o') | Some('O')) {
                s.push(self.cs.next().unwrap()); s.push(self.cs.next().unwrap());
                while let Some(ch) = self.cs.peek() {
                    if ('0'..='7').contains(&ch) || ch == '_' { s.push(ch); self.cs.next(); } else { break; }
                }
                s.push_str(&self.read_num_suffix());
                return (s, false);
            }
        }

        let mut seen_dot = false;
        let mut seen_exp = false;
        while let Some(ch) = self.cs.peek() {
            if ch.is_ascii_digit() || ch == '_' {
                s.push(ch); self.cs.next();
            } else if ch == '.' && !seen_dot && !seen_exp {
                is_float = true; seen_dot = true; s.push(ch); self.cs.next();
            } else if (ch == 'e' || ch == 'E') && !seen_exp {
                let sign = self.cs.peek_ahead(1);
                let d = self.cs.peek_ahead(if matches!(sign, Some('+')|Some('-')) { 2 } else { 1 });
                if d.map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    is_float = true; seen_exp = true;
                    s.push(ch); self.cs.next();
                    if let Some(sigch) = self.cs.peek() {
                        if sigch == '+' || sigch == '-' { s.push(sigch); self.cs.next(); }
                    }
                } else { break; }
            } else {
                break;
            }
        }
        let suffix = self.read_num_suffix();
        if suffix.contains(['f','F']) { is_float = true; }
        s.push_str(&suffix);
        (s, is_float)
    }

    fn read_num_suffix(&mut self) -> String {
        let mut suf = String::new();
        for _ in 0..2 {
            if let Some(ch) = self.cs.peek() {
                if matches!(ch, 'u'|'U'|'l'|'L'|'f'|'F') {
                    suf.push(ch); self.cs.next();
                    continue;
                }
            }
            break;
        }
        suf
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
            if ["==","!=",">=","<=","&&","||","<<",">>","+=","-=","*=","/=","%=","&=","|=","^=","->","::"].contains(&two.as_str()) {
                self.cs.next(); s.push(next); return s;
            }
        }
        s
    }

    pub fn get_token_vec(&self) -> &Vec<Token> { &self.tokens }
    pub fn diagnostics(&self) -> &Vec<Diagnostic> { &self.diagnostics }
}
