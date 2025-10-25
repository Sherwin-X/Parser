#[path = "token.rs"]
mod token;
use token::{Token, TokenType};

#[path = "cstream.rs"]
mod cstream;
use cstream::CStream;

pub struct Scanner {
    cs: CStream, 
    tokens: Vec<Token>,
}

impl Scanner {
    pub fn new(path: &str) -> Self {
        let cs = CStream::from_file(path).expect("failed to read source file");
        Self { cs, tokens: Vec::new() }
    }

    pub fn tokenize(&mut self) {
        while !self.cs.eof() {
            self.skip_whitespace_and_comments();
            if self.cs.eof() { break; }
            let (line, col) = self.cs.position();
            let ch = self.cs.peek().unwrap();

            if ch.is_ascii_alphabetic() || ch == '_' {
                let ident = self.read_identifier();
                let kind = match ident.as_str() {
                    "int" | "float" | "void" | "while" | "return" | "if" | "else" => TokenType::Keyword,
                    _ => TokenType::Identifier,
                };
                self.tokens.push(Token::new(ident, kind, line, col));
            } else if ch.is_ascii_digit() {
                let num = self.read_number();
                let kind = if num.contains('.') { TokenType::FloatConstant } else { TokenType::IntConstant };
                self.tokens.push(Token::new(num, kind, line, col));
            } else if Self::is_operator_start(ch) {
                let op = self.read_operator();
                self.tokens.push(Token::new(op, TokenType::Operator, line, col));
            } else {
                // punctuation or invalid
                let ch = self.cs.next().unwrap();
                let text = ch.to_string();
                let kind = match ch {
                    ';' | ',' | '(' | ')' | '{' | '}' => TokenType::Punctuation,
                    _ => TokenType::Invalid,
                };
                self.tokens.push(Token::new(text, kind, line, col));
            }
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            while let Some(ch) = self.cs.peek() {
                if ch.is_whitespace() { self.cs.next(); } else { break; }
            }
            // comments: // line comments and /* block */
            if self.cs.peek() == Some('/') && self.cs.peek_ahead(1) == Some('/') {
                while let Some(ch) = self.cs.next() { if ch == '\n' { break; } }
                continue;
            }
            if self.cs.peek() == Some('/') && self.cs.peek_ahead(1) == Some('*') {
                // consume '/*'
                self.cs.next(); self.cs.next();
                while let Some(ch) = self.cs.next() {
                    if ch == '*' && self.cs.peek() == Some('/') {
                        self.cs.next(); // consume '/'
                        break;
                    }
                }
                continue;
            }
            break;
        }
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
        while let Some(ch) = self.cs.peek() {
            if ch.is_ascii_digit() {
                s.push(ch);
                self.cs.next();
            } else if ch == '.' && !seen_dot {
                seen_dot = true;
                s.push(ch);
                self.cs.next();
            } else {
                break;
            }
        }
        s
    }

    fn is_operator_start(ch: char) -> bool {
        "+-*/=<>!".contains(ch)
    }

    fn read_operator(&mut self) -> String {
        // handle two-char operators
        let mut s = String::new();
        let first = self.cs.next().unwrap();
        s.push(first);
        if let Some(next) = self.cs.peek() {
            let two = format!("{}{}", first, next);
            if ["==","!=",">=","<="].contains(&two.as_str()) {
                self.cs.next();
                s.push(next);
                return s;
            }
        }
        s
    }

    pub fn get_token_vec(&self) -> &Vec<Token> {
        &self.tokens
    }
}
