/// The semantic category of a highlighted token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    Plain,
    Keyword,
    Type,
    Function,
    String,
    Number,
    Comment,
    Operator,
    Punctuation,
    Constant,
    Attribute,
    Variable,
}
