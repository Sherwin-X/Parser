use crate::cstream::CStream;
use crate::token::{Token, TokenType, Span};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct Diagnostic { pub message: String, pub start: Span }

pub struct Scanner {
    cs: CStream,
    tokens: Vec<Token>,
    keywords: HashSet<String>,
    diagnostics: Vec<Diagnostic>,
}

impl Scanner {
    pub fn new(path: &str) -> Self {
        let cs = CStream::from_file(path).expect("failed to read source file");
        Self::with_stream(cs)
    }
    pub fn from_string(src: String) -> Self { Self::with_stream(CStream::from_string(src)) }

    fn with_stream(cs: CStream) -> Self {
        let defaults = [
            "int","float","void","while","return","if","else","for","do","break","continue",
            "char","double","struct","union","typedef","const","static","switch","case","default",
        ];
        let keywords = defaults.iter().map(|s| s.to_string()).collect();
        Self { cs, tokens: Vec::new(), keywords, diagnostics: Vec::new() }
    }

    pub fn set_keywords(&mut self, kws: impl IntoIterator<Item=String>) { self.keywords = kws.into_iter().collect(); }
    pub fn tokenize(&mut self) { while let Some(t) = self.next_token() { self.tokens.push(t); } }
    pub fn get_token_vec(&self) -> &Vec<Token> { &self.tokens }
    pub fn diagnostics(&self) -> &Vec<Diagnostic> { &self.diagnostics }

    pub fn next_token(&mut self) -> Option<Token> {
        if self.cs.eof() { return None; }

        // 预处理指令（行首的 # ...）
        if self.cs.peek() == Some('#') && self.cs.position().col == 1 {
            let start = self.cs.position();
            let mut s = String::new();
            s.push(self.cs.next().unwrap());
            loop {
                match self.cs.peek() {
                    Some('\\') => {
                        s.push(self.cs.next().unwrap());
                        if let Some(n) = self.cs.peek() {
                            s.push(n);
                            self.cs.next();
                            if n == '\n' { continue; }
                        }
                    }
                    Some(ch) => {
                        s.push(ch);
                        self.cs.next();
                        if ch == '\n' { break; }
                    }
                    None => break,
                }
            }
            return Some(Token::new(s, TokenType::Preprocessor, start, self.cs.position()));
        }

        if let Some(tok) = self.read_ws() { return Some(tok); }
        if self.cs.eof() { return None; }

        let start = self.cs.position();
        let ch = self.cs.peek().unwrap();

        // 注释
        if ch == '/' && self.cs.peek_ahead(1) == Some('/') { return Some(self.read_line_comment()); }
        if ch == '/' && self.cs.peek_ahead(1) == Some('*') { return Some(self.read_block_comment_nested()); }

        // 字面量
        if ch == '"' { return Some(self.read_string()); }
        if ch == '\'' { return Some(self.read_char()); }

        // 标识符/关键字
        if ch.is_ascii_alphabetic() || ch == '_' {
            let ident = self.read_ident();
            let kind = if self.keywords.contains(&ident) { TokenType::Keyword } else { TokenType::Identifier };
            return Some(Token::new(ident, kind, start, self.cs.position()));
        }

        // 数字（支持 .5 / 0xFF / 0b1010 / 0o77）
        if ch.is_ascii_digit() || (ch == '.' && self.cs.peek_ahead(1).map(|d| d.is_ascii_digit()).unwrap_or(false)) {
            let (num, is_float) = self.read_number();
            return Some(Token::new(num, if is_float { TokenType::FloatConstant } else { TokenType::IntConstant }, start, self.cs.position()));
        }

        // 三字符优先（>>=、<<=、...）
        if let Some(tok) = self.read_ellipsis_or_triple_op(start) { return Some(tok); }

        // 运算符/标点
        if self.is_op_punct_starter(ch) {
            let (text, kind) = self.read_op_or_punct();
            return Some(Token::new(text, kind, start, self.cs.position()));
        }

        // 未知字符
        let got = self.cs.next().unwrap();
        self.diagnostics.push(Diagnostic{ message: format!("Unexpected character '{}'", got), start });
        Some(Token::new(got.to_string(), TokenType::Invalid, start, self.cs.position()))
    }

    fn read_ws(&mut self) -> Option<Token> {
        let start = self.cs.position();
        let mut s = String::new();
        let mut any = false;
        while let Some(ch) = self.cs.peek() {
            if ch.is_whitespace() {
                s.push(ch);
                self.cs.next();
                any = true;
            } else { break; }
        }
        any.then(|| Token::new(s, TokenType::Whitespace, start, self.cs.position()))
    }

    fn read_line_comment(&mut self) -> Token {
        let start = self.cs.position();
        let mut s = String::from("//");
        self.cs.next(); self.cs.next();
        while let Some(ch) = self.cs.peek() {
            s.push(ch);
            self.cs.next();
            if ch == '\n' { break; }
        }
        Token::new(s, TokenType::Comment, start, self.cs.position())
    }

    fn read_block_comment_nested(&mut self) -> Token {
        let start = self.cs.position();
        let mut s = String::new();
        s.push(self.cs.next().unwrap()); // '/'
        s.push(self.cs.next().unwrap()); // '*'
        let mut depth = 1usize;
        while !self.cs.eof() {
            if self.cs.peek() == Some('/') && self.cs.peek_ahead(1) == Some('*') {
                s.push(self.cs.next().unwrap()); s.push(self.cs.next().unwrap()); depth += 1; continue;
            }
            if self.cs.peek() == Some('*') && self.cs.peek_ahead(1) == Some('/') {
                s.push(self.cs.next().unwrap()); s.push(self.cs.next().unwrap()); depth -= 1;
                if depth == 0 { break; } else { continue; }
            }
            if let Some(ch) = self.cs.next() { s.push(ch); } else { break; }
        }
        if depth != 0 {
            let end = self.cs.position();
            self.diagnostics.push(Diagnostic{ message: "Unterminated block comment".into(), start });
            return Token::new(s, TokenType::Invalid, start, end);
        }
        Token::new(s, TokenType::Comment, start, self.cs.position())
    }

    fn read_string(&mut self) -> Token {
        let start = self.cs.position();
        let mut s = String::new(); s.push(self.cs.next().unwrap());
        let mut ok = false;
        while let Some(ch) = self.cs.next() {
            s.push(ch);
            if ch == '\\' {
                if let Some(n) = self.cs.peek() {
                    s.push(self.cs.next().unwrap());
                    if n == 'x' {
                        for _ in 0..2 {
                            if let Some(h) = self.cs.peek() {
                                if h.is_ascii_hexdigit() { s.push(self.cs.next().unwrap()); } else { break; }
                            }
                        }
                    } else if n == 'u' && self.cs.peek() == Some('{') {
                        s.push(self.cs.next().unwrap());
                        while let Some(c) = self.cs.peek() {
                            s.push(self.cs.next().unwrap());
                            if c == '}' { break; }
                        }
                    }
                }
                continue;
            }
            if ch == '"' { ok = true; break; }
        }
        if !ok {
            self.diagnostics.push(Diagnostic{ message: "Unterminated string literal".into(), start });
            return Token::new(s, TokenType::Invalid, start, self.cs.position());
        }
        Token::new(s, TokenType::StringLiteral, start, self.cs.position())
    }

    fn read_char(&mut self) -> Token {
        let start = self.cs.position();
        let mut s = String::new(); s.push(self.cs.next().unwrap());
        let mut ok = false;
        while let Some(ch) = self.cs.next() {
            s.push(ch);
            if ch == '\\' {
                if let Some(n) = self.cs.peek() {
                    s.push(self.cs.next().unwrap());
                    if n == 'x' {
                        for _ in 0..2 {
                            if let Some(h) = self.cs.peek() {
                                if h.is_ascii_hexdigit() { s.push(self.cs.next().unwrap()); } else { break; }
                            }
                        }
                    } else if n == 'u' && self.cs.peek() == Some('{') {
                        s.push(self.cs.next().unwrap());
                        while let Some(c) = self.cs.peek() {
                            s.push(self.cs.next().unwrap());
                            if c == '}' { break; }
                        }
                    }
                }
                continue;
            }
            if ch == '\'' { ok = true; break; }
        }
        if !ok {
            self.diagnostics.push(Diagnostic{ message: "Unterminated char literal".into(), start });
            return Token::new(s, TokenType::Invalid, start, self.cs.position());
        }
        Token::new(s, TokenType::CharLiteral, start, self.cs.position())
    }

    fn read_ident(&mut self) -> String {
        let mut s = String::new();
        while let Some(ch) = self.cs.peek() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                s.push(ch); self.cs.next();
            } else { break; }
        }
        s
    }

    fn read_number(&mut self) -> (String, bool) {
        let mut s = String::new();
        let mut is_float = false;

        // 前导 0x/0b/0o
        if self.cs.peek() == Some('0') {
            if let Some(a1) = self.cs.peek_ahead(1) {
                match a1 {
                    'x'|'X' => {
                        s.push(self.cs.next().unwrap()); s.push(self.cs.next().unwrap());
                        while let Some(ch) = self.cs.peek() {
                            if ch.is_ascii_hexdigit() || ch == '_' { s.push(ch); self.cs.next(); } else { break; }
                        }
                        s.push_str(&self.read_suffix());
                        return (s, false);
                    }
                    'b'|'B' => {
                        s.push(self.cs.next().unwrap()); s.push(self.cs.next().unwrap());
                        while let Some(ch) = self.cs.peek() {
                            if ch == '0' || ch == '1' || ch == '_' { s.push(ch); self.cs.next(); } else { break; }
                        }
                        s.push_str(&self.read_suffix());
                        return (s, false);
                    }
                    'o'|'O' => {
                        s.push(self.cs.next().unwrap()); s.push(self.cs.next().unwrap());
                        while let Some(ch) = self.cs.peek() {
                            if ('0'..='7').contains(&ch) || ch == '_' { s.push(ch); self.cs.next(); } else { break; }
                        }
                        s.push_str(&self.read_suffix());
                        return (s, false);
                    }
                    _ => {}
                }
            }
        }

        let mut seen_dot = false;
        let mut seen_exp = false;

        // 支持 .5 形式
        if self.cs.peek() == Some('.') && self.cs.peek_ahead(1).map(|d| d.is_ascii_digit()).unwrap_or(false) {
            is_float = true; seen_dot = true; s.push(self.cs.next().unwrap());
        }

        while let Some(ch) = self.cs.peek() {
            if ch.is_ascii_digit() || ch == '_' {
                s.push(ch); self.cs.next();
            } else if ch == '.' && !seen_dot && !seen_exp {
                is_float = true; seen_dot = true; s.push(ch); self.cs.next();
            } else if (ch == 'e' || ch == 'E') && !seen_exp {
                // 指数部分必须看起来像 e(+|-)d
                let sign = self.cs.peek_ahead(1);
                let d = self.cs.peek_ahead(if matches!(sign, Some('+')|Some('-')) { 2 } else { 1 });
                if d.map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    is_float = true; seen_exp = true; s.push(ch); self.cs.next();
                    if let Some(sig) = self.cs.peek() {
                        if sig == '+' || sig == '-' { s.push(sig); self.cs.next(); }
                    }
                } else { break; }
            } else { break; }
        }

        let suf = self.read_suffix();
        if suf.contains(['f','F']) { is_float = true; }
        s.push_str(&suf);
        (s, is_float)
    }

    fn read_suffix(&mut self) -> String {
        // 最多两位后缀，例如 u/U l/L f/F（粗略处理，够用）
        let mut suf = String::new();
        for _ in 0..2 {
            if let Some(ch) = self.cs.peek() {
                if matches!(ch,'u'|'U'|'l'|'L'|'f'|'F') { suf.push(ch); self.cs.next(); continue; }
            }
            break;
        }
        suf
    }

    fn is_op_punct_starter(&self, ch: char) -> bool {
        "+-*/=<>!%&|^~?:,.(){}[];".contains(ch)
    }

    fn read_ellipsis_or_triple_op(&mut self, start: Span) -> Option<Token> {
        // 省略号 ...
        if self.cs.peek() == Some('.') && self.cs.peek_ahead(1) == Some('.') && self.cs.peek_ahead(2) == Some('.') {
            let mut s = String::new();
            s.push(self.cs.next().unwrap());
            s.push(self.cs.next().unwrap());
            s.push(self.cs.next().unwrap());
            return Some(Token::new(s, TokenType::Punctuation, start, self.cs.position()));
        }
        // 3 字符运算符：>>=、<<=
        let first = self.cs.peek().unwrap();
        let second = self.cs.peek_ahead(1);
        let third = self.cs.peek_ahead(2);
        if let (Some(b), Some(c)) = (second, third) {
            let tri = format!("{}{}{}", first, b, c);
            let tri_ops = ["<<=", ">>="]; // 可按需扩展
            if tri_ops.contains(&tri.as_str()) {
                let mut s = String::new();
                s.push(self.cs.next().unwrap());
                s.push(self.cs.next().unwrap());
                s.push(self.cs.next().unwrap());
                return Some(Token::new(s, TokenType::Operator, start, self.cs.position()));
            }
        }
        None
    }

    fn read_op_or_punct(&mut self) -> (String, TokenType) {
        // 尽量先匹配 2 字符，再退回到 1 字符
        let first = self.cs.next().unwrap();
        let mut s = String::new(); s.push(first);

        if let Some(next) = self.cs.peek() {
            let two = format!("{}{}", first, next);
            // 二字符运算符集合
            let two_ops = [
                "==","!=",">=","<=","&&","||","<<",">>",
                "+=","-=","*=","/=","%=","&=","|=","^=",
                "++","--","->","::",
            ];
            if two_ops.contains(&two.as_str()) {
                self.cs.next(); s.push(next);
                return (s, TokenType::Operator);
            }
        }

        // 单字符：运算符 or 标点
        let kind = match first {
            '('|')'|'{'|'}'|'['|']'|';'|',' => TokenType::Punctuation,
            ':' => TokenType::Operator, // 便于后续三目 ?: 扩展
            '?' => TokenType::Operator,
            _   => TokenType::Operator,
        };
        (s, kind)
    }

    pub fn token_stats(&self) -> HashMap<&'static str, usize> {
        let mut map: HashMap<&'static str, usize> = HashMap::new();
        for t in &self.tokens {
            let k = match t.kind() {
                TokenType::Keyword      => "Keyword",
                TokenType::Identifier   => "Identifier",
                TokenType::IntConstant  => "IntConstant",
                TokenType::FloatConstant=> "FloatConstant",
                TokenType::StringLiteral=> "StringLiteral",
                TokenType::CharLiteral  => "CharLiteral",
                TokenType::Operator     => "Operator",
                TokenType::Punctuation  => "Punctuation",
                TokenType::Comment      => "Comment",
                TokenType::Preprocessor => "Preprocessor",
                TokenType::Whitespace   => "Whitespace",
                TokenType::Invalid      => "Invalid",
                TokenType::None         => "None",
            };
            *map.entry(k).or_insert(0) += 1;
        }
        map
    }
}

// 便于按需流式扫描
pub struct ScannerStream { inner: Scanner }
impl ScannerStream {
    pub fn from_file(path:&str)->Self{ Self{ inner: Scanner::new(path) } }
    pub fn from_string(src:String)->Self{ Self{ inner: Scanner::from_string(src) } }
}
impl Iterator for ScannerStream {
    type Item = Token;
    fn next(&mut self)->Option<Self::Item>{ self.inner.next_token() }
}
