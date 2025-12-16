// scanner.rs
use crate::token::{Token, TokenType, Span};

#[derive(Debug, Clone)]
pub struct LexerError {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub line_text: String,
}

impl LexerError {
    pub fn render(&self) -> String {
        let mut caret = String::new();
        let pos = self.span.col.saturating_sub(1);
        caret.push_str(&" ".repeat(pos));
        caret.push_str(&"^".repeat(self.span.len.max(1)));
        format!(
            "{}: {} at {}:{}\n{}\n{}\n",
            self.code, self.message, self.span.line, self.span.col, self.line_text, caret
        )
    }
}

pub struct Scanner<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    line: usize,  // 1-based
    col: usize,   // 1-based
    tokens: Vec<Token>,
    pub errors: Vec<LexerError>,
}

impl<'a> Scanner<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            line: 1,
            col: 1,
            tokens: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn cur(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    #[inline]
    fn peek_byte(&self, k: usize) -> Option<u8> {
        self.bytes.get(self.pos + k).copied()
    }

    #[inline]
    fn peek_next_is_digit(&self) -> bool {
        matches!(self.peek_byte(1), Some(b) if (b as char).is_ascii_digit())
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.cur()?;
        self.pos += 1;
        if b == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(b)
    }

    fn span_from(&self, start_idx: usize, start_line: usize, start_col: usize) -> Span {
        Span {
            line: start_line,
            col: start_col,
            idx: start_idx,
            len: self.pos.saturating_sub(start_idx),
        }
    }

    fn current_line_text(&self, line: usize) -> String {
        // 简单获取指定行文本（用于报错展示）
        let mut cur_line = 1usize;
        let mut start = 0usize;
        for (i, &b) in self.bytes.iter().enumerate() {
            if b == b'\n' {
                if cur_line == line {
                    return self.src[start..i].to_string();
                }
                cur_line += 1;
                start = i + 1;
            }
        }
        if cur_line == line {
            return self.src[start..].to_string();
        }
        "".into()
    }

    fn push_tok(&mut self, kind: TokenType, text: String, span: Span) {
        self.tokens.push(Token::new(kind, text, span));
    }

    pub fn scan(mut self) -> (Vec<Token>, Vec<LexerError>) {
        while !self.at_end() {
            let b = self.cur().unwrap();
            match b {
                // 空白：不产生 token，但要正确更新位置
                c if (c as char).is_ascii_whitespace() => {
                    self.skip_whitespace();
                }

                // 预处理行：从 '#' 到本行结束
                b'#' if self.is_line_start() => {
                    self.lex_preprocessor();
                }

                // 标识符 / 关键字
                c if (c as char).is_ascii_alphabetic() || c == b'_' => {
                    self.lex_identifier_or_keyword();
                }

                // 数字常量（增强版）
                c if (c as char).is_ascii_digit() => {
                    self.lex_number();
                }

                // 以 '.' 开头的小数，如 .5
                b'.' if self.peek_next_is_digit() => {
                    self.lex_number();
                }

                // 字符/字符串字面量
                b'\'' | b'"' => {
                    self.lex_string_like();
                }

                // 注释或除号
                b'/' => {
                    self.lex_slash_or_comment_or_op();
                }

                // 其它运算符/标点
                _ => {
                    self.lex_operator_or_punct();
                }
            }
        }

        // EOF
        let eof_span = Span {
            line: self.line,
            col: self.col,
            idx: self.pos,
            len: 0,
        };
        self.tokens
            .push(Token::new(TokenType::Eof, String::new(), eof_span));
        (self.tokens, self.errors)
    }

    /* ===================== 子过程 ===================== */

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.cur() {
            if !(c as char).is_ascii_whitespace() {
                break;
            }
            self.bump();
        }
    }

    fn is_line_start(&self) -> bool {
        // 处在一行的第一个非空白字符处
        if self.col != 1 {
            // 若不是 col==1，仍可能前面是空白，这里简单回溯判断
            let mut i = self.pos;
            while i > 0 {
                let b = self.bytes[i - 1];
                if b == b'\n' {
                    break;
                }
                if !(b as char).is_ascii_whitespace() {
                    return false;
                }
                i -= 1;
            }
        }
        true
    }

    fn lex_preprocessor(&mut self) {
        let si = self.pos;
        let sl = self.line;
        let sc = self.col;

        while let Some(b) = self.cur() {
            self.bump();
            if b == b'\n' {
                break;
            }
        }

        let sp = self.span_from(si, sl, sc);
        let text = &self.src[si..self.pos];
        self.push_tok(TokenType::Preprocessor, text.to_string(), sp);
    }

    fn lex_identifier_or_keyword(&mut self) {
        let si = self.pos;
        let sl = self.line;
        let sc = self.col;

        while let Some(b) = self.cur() {
            let ch = b as char;
            if ch.is_ascii_alphanumeric() || ch == '_' {
                self.bump();
            } else {
                break;
            }
        }

        let sp = self.span_from(si, sl, sc);
        let text = &self.src[si..self.pos];

        let kind = if is_keyword(text) {
            TokenType::Keyword
        } else {
            TokenType::Identifier
        };
        self.push_tok(kind, text.to_string(), sp);
    }

    fn lex_number(&mut self) {
        // 支持：
        // - 十进制：123 12.34 1e9 1.2e-3
        // - 以 '.' 开头：.5 .5e2
        // - 十六进制/二进制/八进制整数：0xFF 0b1010 0777
        // - 常见后缀：u/U l/L ll/LL f/F（按“有小数点/指数/f后缀”为 float）
        let si = self.pos;
        let sl = self.line;
        let sc = self.col;

        // 1) 允许以 '.' 开头的小数
        let mut started_with_dot = false;
        if self.cur() == Some(b'.') && self.peek_next_is_digit() {
            started_with_dot = true;
            self.bump(); // eat '.'
        }

        // 2) 基数前缀
        let mut base = 10u32;
        if !started_with_dot && self.cur() == Some(b'0') {
            if let Some(n1) = self.peek_byte(1) {
                match n1 {
                    b'x' | b'X' => {
                        base = 16;
                        self.bump();
                        self.bump();
                    } // 0x
                    b'b' | b'B' => {
                        base = 2;
                        self.bump();
                        self.bump();
                    } // 0b
                    b'o' | b'O' => {
                        base = 8;
                        self.bump();
                        self.bump();
                    } // 0o (扩展)
                    b'0'..=b'7' => {
                        base = 8; /* 0777：不额外 bump，让循环吃掉 */
                    }
                    _ => { /* 0 后面不是合适的前缀/八进制位，按十进制处理 */ }
                }
            }
        }

        // 3) 读取整数部分（对 base!=10 时只吃相应数字）
        let mut saw_digit = started_with_dot;
        while let Some(b) = self.cur() {
            let ch = b as char;
            let ok = match base {
                2 => ch == '0' || ch == '1',
                8 => ch >= '0' && ch <= '7',
                16 => ch.is_ascii_hexdigit(),
                _ => ch.is_ascii_digit(),
            };
            if ok {
                saw_digit = true;
                self.bump();
            } else {
                break;
            }
        }

        // 4) 仅十进制支持小数点与指数（保持实现简单且贴近 C 的常见用法）
        let mut is_float = false;
        if base == 10 && !started_with_dot && self.cur() == Some(b'.') {
            is_float = true;
            self.bump(); // '.'
            while let Some(b) = self.cur() {
                let ch = b as char;
                if ch.is_ascii_digit() {
                    saw_digit = true;
                    self.bump();
                } else {
                    break;
                }
            }
        }

        // 5) 指数：e/E [+/-]? digits+
        if base == 10 {
            if let Some(b) = self.cur() {
                if b == b'e' || b == b'E' {
                    let mark_pos = self.pos;
                    let mark_line = self.line;
                    let mark_col = self.col;

                    is_float = true;
                    self.bump(); // e/E
                    if matches!(self.cur(), Some(b'+') | Some(b'-')) {
                        self.bump();
                    }
                    let mut exp_digits = 0usize;
                    while let Some(b) = self.cur() {
                        if (b as char).is_ascii_digit() {
                            exp_digits += 1;
                            self.bump();
                        } else {
                            break;
                        }
                    }
                    if exp_digits == 0 {
                        // 回退到 e/E 之前位置，并报错（避免吃掉后续 token）
                        self.pos = mark_pos;
                        self.line = mark_line;
                        self.col = mark_col;

                        let sp = self.span_from(si, sl, sc);
                        let line_text = self.current_line_text(sp.line);
                        self.errors.push(LexerError {
                            code: "L1002",
                            message: "malformed exponent (expected digits after e/E)".into(),
                            span: sp,
                            line_text,
                        });
                    }
                }
            }
        }

        // 6) 后缀：u/U, l/L, ll/LL, f/F
        // 简化：只要出现 f/F 就判定为 float
        let mut saw_suffix_f = false;
        while let Some(b) = self.cur() {
            let ch = b as char;
            if matches!(ch, 'u' | 'U' | 'l' | 'L' | 'f' | 'F') {
                if ch == 'f' || ch == 'F' {
                    saw_suffix_f = true;
                }
                self.bump();
            } else {
                break;
            }
        }
        if saw_suffix_f {
            is_float = true;
        }

        // 基本合法性：必须至少看到一个数字（. 之后也算）
        if !saw_digit {
            let sp = self.span_from(si, sl, sc);
            let line_text = self.current_line_text(sp.line);
            self.errors.push(LexerError {
                code: "L1001",
                message: "malformed numeric literal".into(),
                span: sp,
                line_text,
            });
            // 保底前进一个字节避免死循环
            self.bump();
            return;
        }

        let sp = self.span_from(si, sl, sc);
        let text = &self.src[si..self.pos];
        let kind = if is_float {
            TokenType::FloatConstant
        } else {
            TokenType::IntConstant
        };
        self.push_tok(kind, text.to_string(), sp);
    }

    fn lex_string_like(&mut self) {
        let quote = self.cur().unwrap(); // ' or "
        let si = self.pos;
        let sl = self.line;
        let sc = self.col;
        self.bump(); // eat opening quote

        loop {
            if self.at_end() {
                let sp = self.span_from(si, sl, sc);
                let line_text = self.current_line_text(sp.line);
                self.errors.push(LexerError {
                    code: "L3001",
                    message: "unterminated string/char literal".into(),
                    span: sp,
                    line_text,
                });
                break;
            }
            let b = self.cur().unwrap();
            self.bump();
            if b == b'\\' {
                // escape sequence: skip next byte if present
                if !self.at_end() {
                    self.bump();
                }
                continue;
            }
            if b == quote {
                break;
            }
            if b == b'\n' {
                let sp = self.span_from(si, sl, sc);
                let line_text = self.current_line_text(sp.line);
                self.errors.push(LexerError {
                    code: "L3001",
                    message: "unterminated string/char literal".into(),
                    span: sp,
                    line_text,
                });
                break;
            }
        }

        let sp = self.span_from(si, sl, sc);
        let text = &self.src[si..self.pos];
        let kind = if quote == b'\'' {
            TokenType::CharLiteral
        } else {
            TokenType::StringLiteral
        };
        self.push_tok(kind, text.to_string(), sp);
    }

    fn lex_slash_or_comment_or_op(&mut self) {
        let si = self.pos;
        let sl = self.line;
        let sc = self.col;

        self.bump(); // eat '/'

        match self.cur() {
            Some(b'/') => {
                // line comment
                self.bump();
                while let Some(b) = self.cur() {
                    self.bump();
                    if b == b'\n' {
                        break;
                    }
                }
                let sp = self.span_from(si, sl, sc);
                let text = &self.src[si..self.pos];
                self.push_tok(TokenType::Comment, text.to_string(), sp);
            }
            Some(b'*') => {
                // block comment
                self.bump();
                let mut closed = false;
                while let Some(b) = self.cur() {
                    self.bump();
                    if b == b'*' && self.cur() == Some(b'/') {
                        self.bump();
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    let sp = self.span_from(si, sl, sc);
                    let line_text = self.current_line_text(sp.line);
                    self.errors.push(LexerError {
                        code: "L2002",
                        message: "unterminated block comment".into(),
                        span: sp,
                        line_text,
                    });
                }
                let sp = self.span_from(si, sl, sc);
                let text = &self.src[si..self.pos];
                self.push_tok(TokenType::Comment, text.to_string(), sp);
            }
            Some(b'=') => {
                // "/="
                self.bump();
                let sp = self.span_from(si, sl, sc);
                let text = &self.src[si..self.pos];
                self.push_tok(TokenType::Operator, text.to_string(), sp);
            }
            _ => {
                // 单独的 '/'
                let sp = self.span_from(si, sl, sc);
                self.push_tok(TokenType::Operator, "/".into(), sp);
            }
        }
    }

    fn lex_operator_or_punct(&mut self) {
        let si = self.pos;
        let sl = self.line;
        let sc = self.col;

        // 尝试 2 字节运算符
        let two = if self.pos + 1 < self.bytes.len() {
            &self.src[self.pos..self.pos + 2]
        } else {
            ""
        };

        // 尝试 3 字节移位赋值（如 ">>=" "<<="），可选
        let three = if self.pos + 2 < self.bytes.len() {
            &self.src[self.pos..self.pos + 3]
        } else {
            ""
        };

        let (kind, len) = if matches!(three, ">>=" | "<<=") {
            (TokenType::Operator, 3)
        } else if matches!(
            two,
            "->"
                | "++"
                | "--"
                | "&&"
                | "||"
                | "=="
                | "!="
                | "<="
                | ">="
                | "+="
                | "-="
                | "*="
                | "/="
                | "%="
                | "&="
                | "|="
                | "^="
                | "<<"
                | ">>"
        ) {
            (TokenType::Operator, 2)
        } else {
            // 单字符
            let ch = self.cur().unwrap() as char;
            let k = match ch {
                '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | ':' => TokenType::Punctuation,
                '+' | '-' | '*' | '%' | '&' | '|' | '^' | '!' | '~' | '<' | '>' | '=' | '.'
                | '?' => TokenType::Operator,
                _ => {
                    // 未知字符
                    let sp = self.span_from(si, sl, sc);
                    let line_text = self.current_line_text(sp.line);
                    self.errors.push(LexerError {
                        code: "L2001",
                        message: format!("unexpected character '{}'", ch.escape_default()),
                        span: sp,
                        line_text,
                    });
                    self.bump(); // 跳过该字节
                    return;
                }
            };
            (k, 1)
        };

        for _ in 0..len {
            self.bump();
        }
        let sp = self.span_from(si, sl, sc);
        let text = &self.src[si..self.pos];
        self.push_tok(kind, text.to_string(), sp);
    }
}

// 关键字表：按需要扩充
fn is_keyword(s: &str) -> bool {
    matches!(
        s,
        "if"
            | "else"
            | "for"
            | "while"
            | "do"
            | "return"
            | "break"
            | "continue"
            | "switch"
            | "case"
            | "default"
            | "goto"
            | "sizeof"
            | "struct"
            | "union"
            | "enum"
            | "typedef"
            | "static"
            | "extern"
            | "const"
            | "volatile"
            | "signed"
            | "unsigned"
            | "short"
            | "long"
            | "int"
            | "char"
            | "float"
            | "double"
            | "void"
            | "_Bool"
    )
}
