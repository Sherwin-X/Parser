// token.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    Identifier,
    Keyword,
    IntConstant,
    FloatConstant,
    CharLiteral,
    StringLiteral,
    Operator,
    Punctuation,
    Whitespace,
    Comment,
    Preprocessor,
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,   // 1-based
    pub col:  usize,   // 1-based
    pub idx:  usize,   // 起始字节索引
    pub len:  usize,   // 字节长度（用于 ^^^^ 宽度）
}

impl Span {
    pub fn end_col(&self) -> usize {
        self.col + self.len.saturating_sub(1)
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    kind: TokenType,
    text: String,
    span: Span,
}

impl Token {
    pub fn new(kind: TokenType, text: impl Into<String>, span: Span) -> Self {
        Self { kind, text: text.into(), span }
    }
    #[inline] pub fn kind(&self) -> TokenType { self.kind }
    #[inline] pub fn text(&self) -> &str { &self.text }
    #[inline] pub fn span(&self) -> Span { self.span }
}
