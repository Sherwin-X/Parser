#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenType {
    None,
    IntConstant,
    FloatConstant,
    Operator,
    Keyword,
    Identifier,
    Punctuation,
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
}
