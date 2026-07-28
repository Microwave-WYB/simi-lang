use std::{collections::BTreeSet, error::Error, fmt};

use simi_syntax::{
    SyntaxKind, ast,
    generated::{AstNode, Expr as SyntaxExpr, MapEntry, MapExpr, RequiresDecl, Root},
    parse_source,
};

use crate::span::Span;

/// Static dependency metadata declared by a leading `requires` declaration.
///
/// This is source metadata only. Parsing it does not evaluate Simi code or resolve, fetch, or
/// inspect any dependency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requires {
    /// Dependencies in source order.
    pub entries: Vec<Requirement>,
    /// The complete `requires { ... }` declaration.
    pub span: Span,
}

/// One named requirement from a [`Requires`] declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirement {
    /// The lowercase identifier used to refer to this dependency.
    pub alias: String,
    /// The requirement's static source specification.
    pub source: RequirementSource,
}

/// A static dependency source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequirementSource {
    /// An immutable Git revision.
    Git { git: String, rev: String },
    /// A package-root-relative development path.
    Path { path: String },
}

/// A static-requirements syntax or validation error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageRequirementsError {
    /// The source range responsible for the error.
    pub span: Span,
    /// A human-readable explanation of the rejected static data.
    pub message: String,
}

impl fmt::Display for PackageRequirementsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for PackageRequirementsError {}

/// Extract and validate a leading `requires` declaration without evaluating source.
///
/// `Ok(None)` means the source has no declaration. The source must otherwise be valid Simi
/// syntax, and every requirement must be either `{git = string, rev = string}` or
/// `{path = string}`.
pub fn parse_requires(source: &str) -> Result<Option<Requires>, PackageRequirementsError> {
    let parse = parse_source(source);
    let root = Root::cast(parse.syntax().clone()).expect("parser produces a root node");
    let declaration = root.syntax().children().find_map(RequiresDecl::cast);
    let declaration_span = declaration
        .as_ref()
        .map(|declaration| node_span(declaration.syntax()));
    if let Some(diagnostic) = parse.diagnostics().iter().find(|diagnostic| {
        !matches!(
            declaration_span,
            Some(span)
                if diagnostic.message.starts_with("duplicate map field `")
                    && span.start <= diagnostic.span.start
                    && diagnostic.span.end <= span.end
        )
    }) {
        return Err(error(diagnostic.span, diagnostic.message.clone()));
    }

    let Some(declaration) = declaration else {
        return Ok(None);
    };
    let map = ast::child::<MapExpr>(declaration.syntax())
        .expect("validated requires declaration contains a map");
    let entries = fields(map.syntax(), "requires declaration")?
        .into_iter()
        .map(parse_requirement)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Some(Requires {
        entries,
        span: node_span(declaration.syntax()),
    }))
}

struct Field {
    name: String,
    name_span: Span,
    value: SyntaxExpr,
}

fn fields(
    node: &simi_syntax::SyntaxNode,
    context: &str,
) -> Result<Vec<Field>, PackageRequirementsError> {
    let mut names = BTreeSet::new();
    let mut fields = Vec::new();
    for entry in node.children().filter_map(MapEntry::cast) {
        let entry_span = node_span(entry.syntax());
        let Some(name_token) = ast::token(entry.syntax(), SyntaxKind::IDENT) else {
            return Err(error(
                entry_span,
                format!("{context} keys must be identifier aliases"),
            ));
        };
        let name = name_token.text().to_string();
        let name_span = token_span(&name_token);
        if !names.insert(name.clone()) {
            return Err(error(
                name_span,
                format!("{context} declares alias or field `{name}` more than once"),
            ));
        }
        let Some(value) = entry
            .syntax()
            .children()
            .filter_map(SyntaxExpr::cast)
            .next()
        else {
            return Err(error(
                entry_span,
                format!("{context} fields require a value"),
            ));
        };
        fields.push(Field {
            name,
            name_span,
            value,
        });
    }
    Ok(fields)
}

fn parse_requirement(field: Field) -> Result<Requirement, PackageRequirementsError> {
    if !valid_alias(&field.name) {
        return Err(error(
            field.name_span,
            format!(
                "requirement alias `{}` must be a lowercase Simi identifier",
                field.name
            ),
        ));
    }

    let SyntaxExpr::Map(map) = field.value else {
        return Err(error(
            field.name_span,
            format!("requirement `{}` must be a static map", field.name),
        ));
    };
    let source_fields = fields(map.syntax(), &format!("requirement `{}`", field.name))?;
    let mut git = None;
    let mut rev = None;
    let mut path = None;

    for source_field in source_fields {
        let value = static_string(
            &source_field.value,
            &format!("requirement `{}` field `{}`", field.name, source_field.name),
        )?;
        match source_field.name.as_str() {
            "git" => git = Some(value),
            "rev" => rev = Some(value),
            "path" => path = Some((value, source_field.name_span)),
            _ => {
                return Err(error(
                    source_field.name_span,
                    format!(
                        "requirement `{}` does not permit field `{}`",
                        field.name, source_field.name
                    ),
                ));
            }
        }
    }

    let source = match (git, rev, path) {
        (Some(git), Some(rev), None) if !git.is_empty() && !rev.is_empty() => {
            RequirementSource::Git { git, rev }
        }
        (None, None, Some((path, path_span))) => {
            validate_development_path(&path, path_span)?;
            RequirementSource::Path { path }
        }
        (_, _, Some(_)) => {
            return Err(error(
                field.name_span,
                format!(
                    "requirement `{}` cannot mix `path` with `git` or `rev`",
                    field.name
                ),
            ));
        }
        _ => {
            return Err(error(
                field.name_span,
                format!(
                    "requirement `{}` must declare either `git` and `rev`, or `path`",
                    field.name
                ),
            ));
        }
    };

    Ok(Requirement {
        alias: field.name,
        source,
    })
}

fn static_string(
    expression: &SyntaxExpr,
    context: &str,
) -> Result<String, PackageRequirementsError> {
    let SyntaxExpr::Literal(literal) = expression else {
        return Err(error(
            node_span(expression.syntax()),
            format!("{context} must be a string literal"),
        ));
    };
    let Some(token) = ast::token(literal.syntax(), SyntaxKind::STRING) else {
        return Err(error(
            node_span(expression.syntax()),
            format!("{context} must be a string literal"),
        ));
    };
    Ok(decode_string(token.text()))
}

fn validate_development_path(path: &str, span: Span) -> Result<(), PackageRequirementsError> {
    let windows_absolute = path.len() >= 3
        && path.as_bytes()[0].is_ascii_alphabetic()
        && path.as_bytes()[1] == b':'
        && matches!(path.as_bytes()[2], b'/' | b'\\');
    if path.is_empty()
        || path.starts_with('/')
        || windows_absolute
        || path.contains('\\')
        || path
            .split('/')
            .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
    {
        return Err(error(
            span,
            "development path must be a non-escaping, package-root-relative slash-separated path",
        ));
    }
    Ok(())
}

fn valid_alias(alias: &str) -> bool {
    let mut characters = alias.chars();
    matches!(characters.next(), Some('a'..='z'))
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
}

fn decode_string(text: &str) -> String {
    let body = text
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .expect("parser validates string literals");
    let mut result = String::new();
    let mut characters = body.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            result.push(character);
            continue;
        }
        result.push(match characters.next().expect("parser validates escapes") {
            '"' => '"',
            '\\' => '\\',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            _ => unreachable!("parser validates escapes"),
        });
    }
    result
}

fn node_span(node: &simi_syntax::SyntaxNode) -> Span {
    let range = node.text_range();
    Span::new(
        u32::from(range.start()) as usize,
        u32::from(range.end()) as usize,
    )
}

fn token_span(token: &simi_syntax::SyntaxToken) -> Span {
    let range = token.text_range();
    Span::new(
        u32::from(range.start()) as usize,
        u32::from(range.end()) as usize,
    )
}

fn error(span: Span, message: impl Into<String>) -> PackageRequirementsError {
    PackageRequirementsError {
        span,
        message: message.into(),
    }
}
