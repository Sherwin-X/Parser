#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span { pub line: usize, pub col: usize, pub offset: usize }

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TokenType { None, IntConstant, FloatConstant, StringLiteral, CharLiteral, Operator, Keyword, Identifier, Punctuation, Comment, Preprocessor, Whitespace, Invalid }

#[derive(Debug, Clone)]
pub struct Token { text: String, token_type: TokenType, start: Span, end: Span }
impl Token { 
    pub fn new(text: String, token_type: TokenType, start: Span, end: Span) -> Self { Self { text, token_type, start, end } }
    pub fn text(&self) -> &str { &self.text }
    pub fn kind(&self) -> &TokenType { &self.token_type }
    pub fn start(&self) -> &Span { &self.start }
    pub fn end(&self) -> &Span { &self.end }
    pub fn css_class(&self) -> &'static str {
        match self.token_type {
            TokenType::Keyword => "kw", TokenType::Identifier => "id",
            TokenType::IntConstant | TokenType::FloatConstant => "num",
            TokenType::StringLiteral | TokenType::CharLiteral => "str",
            TokenType::Operator => "op", TokenType::Punctuation => "punct",
            TokenType::Comment => "cmt", TokenType::Preprocessor => "pp",
            TokenType::Whitespace => "ws", TokenType::Invalid => "err",
            TokenType::None => "tok",
        }
    }
}
