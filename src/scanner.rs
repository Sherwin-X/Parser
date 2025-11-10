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
            "{code}: {msg} at {line}:{col}\n{src}\n{caret}\n",
            code = self.code,
            msg  = self.message,
            line = self.span.line,
            col  = self.span.col,
            src  = self.line_text,
            caret = caret
        )
    }
}

pub struct Scanner<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,   // 当前字节索引
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

    #[inline] fn at_end(&self) -> bool { self.pos >= self.bytes.len() }
    #[inline] fn cur(&self) -> Option<u8> { self.bytes.get(self.pos).copied() }
    #[inline] fn peek_n(&self, n: usize) -> Option<u8> { self.bytes.get(self.pos + n).copied() }

    fn bump(&mut self) -> Option<u8> {
        if self.at_end() { return None; }
        let b = self.bytes[self.pos];
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
            col:  start_col,
            idx:  start_idx,
            len:  self.pos.saturating_sub(start_idx),
        }
    }

    fn current_line_text(&self, line: usize) -> String {
        let bs = self.bytes;
        let mut cur = 1usize;
        let mut start = 0usize;
        for i in 0..=bs.len() {
            if i == bs.len() || bs[i] == b'\n' {
                if cur == line {
                    return String::from_utf8_lossy(&bs[start..i]).into_owned();
                }
                cur += 1;
                start = i + 1;
            }
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
                c if (c as char).is_ascii_whitespace() => { self.skip_whitespace(); }

                // 预处理行：从 '#' 到本行结束
                b'#' if self.is_line_start() => { self.lex_preprocessor(); }

                // 标识符 / 关键字
                c if (c as char).is_ascii_alphabetic() || c == b'_' => { self.lex_identifier_or_keyword(); }

                // 数字常量（简单版：十进制 + 小数点）
                c if (c as char).is_ascii_digit() => { self.lex_number(); }

                // 字符/字符串字面量
                b'\'' | b'"' => { self.lex_string_like(); }

                // 注释或除号
                b'/' => { self.lex_slash_or_comment_or_op(); }

                // 其它运算符/标点
                _ => { self.lex_operator_or_punct(); }
            }
        }

        // EOF
        let eof_span = Span { line: self.line, col: self.col, idx: self.pos, len: 0 };
        self.tokens.push(Token::new(TokenType::Eof, String::new(), eof_span));
        (self.tokens, self.errors)
    }

    /* ===================== 子过程 ===================== */

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.cur() {
            if !(c as char).is_ascii_whitespace() { break; }
            self.bump();
        }
    }

    fn is_line_start(&self) -> bool {
        // 处在一行的第一个非空白字符处
        if self.col != 1 {
            // 若不是 col==1，仍可能前面是空白，这里简单回溯判断
            let mut i = self.pos;
            while i > 0 {
                let b = self.bytes[i-1];
                if b == b'\n' { break; }
                if !(b as char).is_ascii_whitespace() { return false; }
                i -= 1;
            }
        }
        true
    }

    fn lex_preprocessor(&mut self) {
        let si = self.pos;
        let sl = self.line;
        let sc = self.col;
        // 读到行尾
        while let Some(c) = self.cur() {
            self.bump();
            if c == b'\n' { break; }
        }
        let sp = self.span_from(si, sl, sc);
        let text = &self.src[si..self.pos];
        self.push_tok(TokenType::Preprocessor, text.to_string(), sp);
    }

    fn lex_identifier_or_keyword(&mut self) {
        let si = self.pos;
        let sl = self.line;
        let sc = self.col;

        while let Some(c) = self.cur() {
            if (c as char).is_ascii_alphanumeric() || c == b'_' {
                self.bump();
            } else { break; }
        }

        let sp = self.span_from(si, sl, sc);
        let text = &self.src[si..self.pos];

        let kw = matches!(text,
            "int" | "float" | "double" | "char" | "void" |
            "if" | "else" | "for" | "while" | "return" |
            "switch" | "case" | "default" | "break" | "continue" |
            "sizeof"
        );

        let kind = if kw { TokenType::Keyword } else { TokenType::Identifier };
        self.push_tok(kind, text.to_string(), sp);
    }

    fn lex_number(&mut self) {
        let si = self.pos;
        let sl = self.line;
        let sc = self.col;
        let mut has_dot = false;

        while let Some(c) = self.cur() {
            let ch = c as char;
            if ch.is_ascii_digit() {
                self.bump();
            } else if ch == '.' && !has_dot {
                has_dot = true;
                self.bump();
            } else {
                break;
            }
        }

        let sp = self.span_from(si, sl, sc);
        let text = &self.src[si..self.pos];
        let kind = if has_dot { TokenType::FloatConstant } else { TokenType::IntConstant };
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
                self.errors.push(LexerError{
                    code: "L3001",
                    message: "unterminated string/char literal".into(),
                    span: sp,
                    line_text,
                });
                return;
            }
            let c = self.cur().unwrap();

            if c == quote {
                self.bump(); // closing quote
                let sp = self.span_from(si, sl, sc);
                let text = &self.src[si..self.pos];
                let kind = if quote == b'"' { TokenType::StringLiteral } else { TokenType::CharLiteral };
                self.push_tok(kind, text.to_string(), sp);
                return;
            }

            if c == b'\\' {
                // 转义：跳过反斜杠与下一个字符
                self.bump();
                if self.at_end() {
                    let sp = self.span_from(si, sl, sc);
                    let line_text = self.current_line_text(sp.line);
                    self.errors.push(LexerError{
                        code: "L3003",
                        message: "unterminated escape sequence".into(),
                        span: sp,
                        line_text,
                    });
                    return;
                }
                self.bump();
                continue;
            }

            if c == b'\n' {
                // 字符串/字符字面量不得跨行（除非使用 '\' 换行，这里不支持）
                let sp = self.span_from(si, sl, sc);
                let line_text = self.current_line_text(sp.line);
                self.errors.push(LexerError{
                    code: "L3001",
                    message: "unterminated string/char literal".into(),
                    span: sp,
                    line_text,
                });
                return;
            }

            self.bump();
        }
    }

    fn lex_slash_or_comment_or_op(&mut self) {
        let si = self.pos;
        let sl = self.line;
        let sc = self.col;

        self.bump(); // consume '/'

        match self.cur() {
            Some(b'/') => {
                // line comment
                while let Some(c) = self.cur() {
                    self.bump();
                    if c == b'\n' { break; }
                }
                let sp = self.span_from(si, sl, sc);
                let text = &self.src[si..self.pos];
                self.push_tok(TokenType::Comment, text.to_string(), sp);
            }
            Some(b'*') => {
                // block comment
                self.bump(); // eat '*'
                let mut closed = false;
                while !self.at_end() {
                    let c = self.bump().unwrap();
                    if c == b'*' && self.cur() == Some(b'/') {
                        self.bump(); // eat '/'
                        closed = true;
                        break;
                    }
                }
                let sp = self.span_from(si, sl, sc);
                if closed {
                    let text = &self.src[si..self.pos];
                    self.push_tok(TokenType::Comment, text.to_string(), sp);
                } else {
                    let line_text = self.current_line_text(sp.line);
                    self.errors.push(LexerError{
                        code: "L3002",
                        message: "unterminated block comment".into(),
                        span: sp,
                        line_text,
                    });
                }
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
            &self.src[self.pos..self.pos+2]
        } else { "" };

        // 尝试 3 字节移位赋值（如 ">>=" "<<="），可选
        let three = if self.pos + 2 < self.bytes.len() {
            &self.src[self.pos..self.pos+3]
        } else { "" };

        let (kind, len) = if matches!(three, ">>=" | "<<=") {
            (TokenType::Operator, 3)
        } else if matches!(two, "->" | "++" | "--" | "&&" | "||" | "==" | "!=" | "<=" | ">=" |
                                "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<" | ">>") {
            (TokenType::Operator, 2)
        } else {
            // 单字符
            let ch = self.cur().unwrap() as char;
            let k = match ch {
                '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | ':' => TokenType::Punctuation,
                '+' | '-' | '*' | '%' | '&' | '|' | '^' | '!' | '~' | '<' | '>' | '=' | '.' | '?' => TokenType::Operator,
                _ => {
                    // 未知字符
                    let sp = self.span_from(si, sl, sc);
                    let line_text = self.current_line_text(sp.line);
                    self.errors.push(LexerError{
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

        for _ in 0..len { self.bump(); }
        let sp = self.span_from(si, sl, sc);
        let text = &self.src[si..self.pos];
        self.push_tok(kind, text.to_string(), sp);
    }
}
