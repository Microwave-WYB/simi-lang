use rowan::Language;

pub use crate::generated::SyntaxKind;

impl SyntaxKind {
    pub const fn is_trivia(self) -> bool {
        matches!(self, Self::WHITESPACE | Self::COMMENT)
    }

    pub const fn is_expression(self) -> bool {
        matches!(
            self,
            Self::LITERAL_EXPR
                | Self::NAME_EXPR
                | Self::FUNCTION_EXPR
                | Self::BLOCK_EXPR
                | Self::PAREN_EXPR
                | Self::LIST_EXPR
                | Self::MAP_EXPR
                | Self::CALL_EXPR
                | Self::FIELD_EXPR
                | Self::INDEX_EXPR
                | Self::NIL_PROPAGATE_EXPR
                | Self::UNARY_EXPR
                | Self::BINARY_EXPR
                | Self::ASSIGN_EXPR
                | Self::PIPELINE_EXPR
                | Self::TRAILING_ARGUMENT_EXPR
                | Self::RAISE_EXPR
                | Self::TRY_EXPR
                | Self::CASE_EXPR
                | Self::IF_EXPR
                | Self::LOOP_EXPR
                | Self::CONTINUE_EXPR
                | Self::BREAK_EXPR
        )
    }

    pub const fn is_pattern(self) -> bool {
        matches!(
            self,
            Self::BINDING_PATTERN
                | Self::WILDCARD_PATTERN
                | Self::LITERAL_PATTERN
                | Self::LIST_PATTERN
                | Self::MAP_PATTERN
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SimiLanguage {}

impl Language for SimiLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        assert!(raw.0 <= SyntaxKind::ERROR as u16);
        // SAFETY: SyntaxKind is repr(u16), contiguous, and bounded above.
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw.0) }
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

pub type SyntaxNode = rowan::SyntaxNode<SimiLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<SimiLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<SimiLanguage>;
