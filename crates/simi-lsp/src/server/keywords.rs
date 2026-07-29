use simi_syntax::{SyntaxKind as K, SyntaxToken};

use super::*;

#[derive(Clone, Copy)]
pub(super) struct KeywordHelp {
    pub word: &'static str,
    pub syntax: &'static str,
    pub documentation: &'static str,
    contextual: bool,
}

const KEYWORDS: &[KeywordHelp] = &[
    KeywordHelp {
        word: "fn",
        syntax: "fn name(parameters) expression",
        documentation: "Declares a named function. Without a name, creates an anonymous function expression.",
        contextual: false,
    },
    KeywordHelp {
        word: "do",
        syntax: "do … end",
        documentation: "Begins a value-producing standalone block, explicit multi-item body, or protected expression when followed by `catch`.",
        contextual: false,
    },
    KeywordHelp {
        word: "end",
        syntax: "… end",
        documentation: "Closes the nearest function, block, conditional, case, or protected expression.",
        contextual: false,
    },
    KeywordHelp {
        word: "if",
        syntax: "if condition then … end",
        documentation: "Begins an expression-valued conditional. Conditions must be boolean.",
        contextual: false,
    },
    KeywordHelp {
        word: "then",
        syntax: "if condition then …",
        documentation: "Begins the selected body of an `if` or `elseif` condition.",
        contextual: false,
    },
    KeywordHelp {
        word: "elseif",
        syntax: "elseif condition then …",
        documentation: "Adds another condition and value-producing branch to an `if` expression.",
        contextual: false,
    },
    KeywordHelp {
        word: "else",
        syntax: "else …",
        documentation: "Provides the fallback branch of an `if` expression.",
        contextual: false,
    },
    KeywordHelp {
        word: "let",
        syntax: "let pattern = value",
        documentation: "Evaluates a value once, matches it against a pattern, and introduces new lexical bindings.",
        contextual: false,
    },
    KeywordHelp {
        word: "alias",
        syntax: "alias name = type",
        documentation: "Declares a transparent, runtime-erased type alias.",
        contextual: false,
    },
    KeywordHelp {
        word: "type",
        syntax: "type name = union",
        documentation: "Declares a named, recursive, runtime-erased structural type.",
        contextual: true,
    },
    KeywordHelp {
        word: "requires",
        syntax: "requires {name = {git = url, rev = revision}}",
        documentation: "Declares static package requirements before executable source items. Use `{git = url, rev = revision}` for Git packages or `{path = path}` for development packages. The runtime supplies portable `std/*` modules; do not declare `std` here.",
        contextual: false,
    },
    KeywordHelp {
        word: "tap",
        syntax: "value |> tap call()",
        documentation: "Runs a pipeline stage for its effects, discards the call result, and preserves the incoming value.",
        contextual: false,
    },
    KeywordHelp {
        word: "nil",
        syntax: "nil",
        documentation: "The absence value. Missing branches and empty blocks also evaluate to `nil`.",
        contextual: false,
    },
    KeywordHelp {
        word: "true",
        syntax: "true",
        documentation: "The boolean true value.",
        contextual: false,
    },
    KeywordHelp {
        word: "false",
        syntax: "false",
        documentation: "The boolean false value.",
        contextual: false,
    },
    KeywordHelp {
        word: "and",
        syntax: "left and right",
        documentation: "Strict boolean conjunction. The right operand is evaluated only when the left operand is true.",
        contextual: false,
    },
    KeywordHelp {
        word: "or",
        syntax: "left or right",
        documentation: "Strict boolean disjunction. The right operand is evaluated only when the left operand is false.",
        contextual: false,
    },
    KeywordHelp {
        word: "not",
        syntax: "not value",
        documentation: "Strict boolean negation.",
        contextual: false,
    },
    KeywordHelp {
        word: "case",
        syntax: "case value of pattern => expression … end",
        documentation: "Begins expression-valued structural pattern matching. One `of` introduces one or more pattern-result arms.",
        contextual: false,
    },
    KeywordHelp {
        word: "of",
        syntax: "of pattern when guard => expression",
        documentation: "Introduces arms for a `case` expression. Every arm uses `=>`, and an optional guard must evaluate to boolean.",
        contextual: false,
    },
    KeywordHelp {
        word: "when",
        syntax: "pattern when guard => expression",
        documentation: "Adds a boolean guard before a `case` or `catch` arm's `=>`.",
        contextual: false,
    },
    KeywordHelp {
        word: "raise",
        syntax: "raise value",
        documentation: "Raises any Simi value through the catchable language-error channel.",
        contextual: false,
    },
    KeywordHelp {
        word: "panic",
        syntax: "panic \"reason\"",
        documentation: "Stops evaluation with an uncatchable hard diagnostic. The optional reason must be a string.",
        contextual: false,
    },
    KeywordHelp {
        word: "todo",
        syntax: "todo \"note\"",
        documentation: "Marks unfinished code, reports an analyzer warning, and stops evaluation with an uncatchable hard diagnostic.",
        contextual: false,
    },
    KeywordHelp {
        word: "catch",
        syntax: "do … catch pattern => expression … end",
        documentation: "Begins the catch section for values raised by the protected `do` body. Patterns and result arms follow directly without `of`.",
        contextual: false,
    },
    KeywordHelp {
        word: "any",
        syntax: "any",
        documentation: "The explicit dynamic escape type. Operations on `any` defer static checking.",
        contextual: true,
    },
    KeywordHelp {
        word: "never",
        syntax: "never",
        documentation: "The bottom type for expressions that do not complete normally.",
        contextual: true,
    },
    KeywordHelp {
        word: "boolean",
        syntax: "boolean",
        documentation: "The static type of `true` and `false`.",
        contextual: true,
    },
    KeywordHelp {
        word: "integer",
        syntax: "integer",
        documentation: "The static type of integer values.",
        contextual: true,
    },
    KeywordHelp {
        word: "float",
        syntax: "float",
        documentation: "The static type of finite floating-point values.",
        contextual: true,
    },
    KeywordHelp {
        word: "string",
        syntax: "string",
        documentation: "The static type of string values.",
        contextual: true,
    },
];

pub(super) fn completions(prefix: &str) -> impl Iterator<Item = &'static KeywordHelp> {
    KEYWORDS.iter().filter(move |keyword| {
        prefix.is_empty() || keyword.word.starts_with(prefix) || keyword.word.contains(prefix)
    })
}

pub(super) fn at(
    db: &AnalysisDatabase,
    file: FileId,
    offset: usize,
) -> Option<(&'static KeywordHelp, Span)> {
    let parsed = simi_analysis::parse(db, file);
    let token = parsed
        .syntax()
        .token_at_offset((offset as u32).into())
        .find(|token| contains(token, offset))?;
    let keyword = KEYWORDS
        .iter()
        .find(|keyword| keyword.word == token.text())?;
    if keyword.contextual && !is_contextual_keyword(&token, keyword.word) {
        return None;
    }
    let range = token.text_range();
    Some((keyword, Span::new(range.start().into(), range.end().into())))
}

pub(super) fn hover_text(keyword: &KeywordHelp) -> String {
    format!(
        "keyword `{}`\n\n{}\n\nSyntax: {}",
        keyword.word, keyword.documentation, keyword.syntax
    )
}

fn contains(token: &SyntaxToken, offset: usize) -> bool {
    let range = token.text_range();
    let start: u32 = range.start().into();
    let end: u32 = range.end().into();
    start as usize <= offset && offset < end as usize
}

fn is_contextual_keyword(token: &SyntaxToken, word: &str) -> bool {
    token.parent_ancestors().any(|node| match word {
        "type" => node.kind() == K::TYPE_DECL,
        "any" | "never" | "boolean" | "integer" | "float" | "string" => matches!(
            node.kind(),
            K::TYPE_EXPR
                | K::TYPE_NAME
                | K::TYPE_PRIMARY
                | K::TYPE_ANNOTATION
                | K::RETURN_ANNOTATION
        ),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_every_language_and_type_keyword() {
        let actual = KEYWORDS
            .iter()
            .map(|keyword| keyword.word)
            .collect::<BTreeSet<_>>();
        let expected = [
            "alias", "and", "any", "boolean", "case", "catch", "do", "else", "elseif", "end",
            "false", "float", "fn", "if", "integer", "let", "nil", "not", "of", "or", "panic",
            "raise", "requires", "string", "tap", "then", "todo", "true", "type", "when", "never",
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        assert_eq!(actual, expected);
        assert!(
            KEYWORDS
                .iter()
                .all(|keyword| { !keyword.syntax.is_empty() && !keyword.documentation.is_empty() })
        );
    }
}
