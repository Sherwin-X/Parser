#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenType {
    None,
    IntConstant,
    FloatConstant,
    StringLiteral,
    CharLiteral,
    Operator,
    Keyword,
    Identifier,
    Punctuation,
    Comment,
    Whitespace,
    Invalid,
}

#[derive(Debug, Clone)]
pub struct Token {
    text: String,
    token_type: TokenType,
    line: usize,
    col: usize,
}

impl Token {
    pub fn new(text: String, token_type: TokenType, line: usize, col: usize) -> Self {
        Self { text, token_type, line, col }
    }
    pub fn text(&self) -> &str { &self.text }
    pub fn kind(&self) -> &TokenType { &self.token_type }
    pub fn line(&self) -> usize { self.line }
    pub fn col(&self) -> usize { self.col }

    pub fn css_class(&self) -> &'static str {
        match self.token_type {
            TokenType::Keyword => "kw",
            TokenType::Identifier => "id",
            TokenType::IntConstant | TokenType::FloatConstant => "num",
            TokenType::StringLiteral | TokenType::CharLiteral => "str",
            TokenType::Operator => "op",
            TokenType::Punctuation => "punct",
            TokenType::Comment => "cmt",
            TokenType::Whitespace => "ws",
            TokenType::Invalid => "err",
            TokenType::None => "tok",
        }
    }
}
