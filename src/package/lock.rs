use std::collections::BTreeMap;

use simi_syntax::{
    SyntaxKind, ast,
    generated::{AstNode, Expr as SyntaxExpr, ExprStmt, MapEntry, MapExpr, Root, Stmt},
    parse_source,
};

use super::RequirementSource;

pub(super) const LOCK_FORMAT: u64 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LockFile {
    pub source: LockSource,
    pub requirements: BTreeMap<String, LockedRequirement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LockSource {
    pub path: String,
    pub digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LockedRequirement {
    pub source: RequirementSource,
    pub package: String,
    pub commit: Option<String>,
    pub tree_digest: String,
}

#[derive(Clone)]
struct Field {
    name: String,
    value: SyntaxExpr,
}

impl LockFile {
    pub fn parse(source: &str) -> Result<Self, String> {
        let parse = parse_source(source);
        if let Some(diagnostic) = parse.diagnostics().first() {
            return Err(diagnostic.message.clone());
        }
        let root = Root::cast(parse.syntax().clone()).expect("parser produces a root node");
        let statements = root.statements().collect::<Vec<_>>();
        let [Stmt::ExprStmt(statement)] = statements.as_slice() else {
            return Err("lockfile must contain exactly one map expression".to_owned());
        };
        let map = expression(statement)?;
        let SyntaxExpr::Map(map) = map else {
            return Err("lockfile must contain exactly one map expression".to_owned());
        };
        let root_fields = fields(map.syntax(), "lockfile")?;
        reject_unknown(
            &root_fields,
            &["format", "source", "requirements"],
            "lockfile",
        )?;
        let format = required_integer(&root_fields, "format", "lockfile")?;
        if format != LOCK_FORMAT {
            return Err(format!("lockfile format must be {LOCK_FORMAT}"));
        }
        let source = parse_source_entry(required_map(&root_fields, "source", "lockfile")?)?;
        let requirements = fields(
            required_map(&root_fields, "requirements", "lockfile")?.syntax(),
            "lockfile requirements",
        )?
        .into_iter()
        .map(|field| {
            let name = field.name;
            let SyntaxExpr::Map(value) = field.value else {
                return Err(format!("lockfile requirement `{name}` must be a map"));
            };
            Ok((name, parse_requirement(value)?))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
        Ok(Self {
            source,
            requirements,
        })
    }

    pub fn render(&self) -> String {
        let mut output = String::from("{\n");
        output.push_str(&format!("    format = {LOCK_FORMAT},\n"));
        output.push_str("    source = {path = ");
        output.push_str(&string(&self.source.path));
        output.push_str(", digest = ");
        output.push_str(&string(&self.source.digest));
        output.push_str("},\n    requirements = {");
        if !self.requirements.is_empty() {
            output.push('\n');
            for (name, requirement) in &self.requirements {
                output.push_str("        ");
                render_map_key(&mut output, name);
                output.push_str(" = {source = ");
                render_requirement_source(&mut output, &requirement.source);
                output.push_str(", package = ");
                output.push_str(&string(&requirement.package));
                if let Some(commit) = &requirement.commit {
                    output.push_str(", commit = ");
                    output.push_str(&string(commit));
                }
                output.push_str(", tree_digest = ");
                output.push_str(&string(&requirement.tree_digest));
                output.push_str("},\n");
            }
            output.push_str("    ");
        }
        output.push_str("},\n}\n");
        output
    }
}

fn parse_source_entry(map: MapExpr) -> Result<LockSource, String> {
    let fields = fields(map.syntax(), "lockfile source")?;
    reject_unknown(&fields, &["path", "digest"], "lockfile source")?;
    Ok(LockSource {
        path: required_string(&fields, "path", "lockfile source")?,
        digest: required_string(&fields, "digest", "lockfile source")?,
    })
}

fn parse_requirement(map: MapExpr) -> Result<LockedRequirement, String> {
    let fields = fields(map.syntax(), "lockfile requirement")?;
    reject_unknown(
        &fields,
        &["source", "package", "commit", "tree_digest"],
        "lockfile requirement",
    )?;
    let source =
        parse_requirement_source(required_map(&fields, "source", "lockfile requirement")?)?;
    let package = required_string(&fields, "package", "lockfile requirement")?;
    let tree_digest = required_string(&fields, "tree_digest", "lockfile requirement")?;
    let commit = fields
        .iter()
        .find(|field| field.name == "commit")
        .map(|field| static_string(&field.value, "lockfile requirement field `commit`"))
        .transpose()?;
    match (&source, &commit) {
        (RequirementSource::Git { .. }, Some(_)) => {}
        (RequirementSource::Git { .. }, None) => {
            return Err("a Git lockfile requirement requires `commit`".to_owned());
        }
        (RequirementSource::Path { .. }, None) => {}
        (RequirementSource::Path { .. }, Some(_)) => {
            return Err("a path lockfile requirement must not contain `commit`".to_owned());
        }
    }
    Ok(LockedRequirement {
        source,
        package,
        commit,
        tree_digest,
    })
}

fn parse_requirement_source(map: MapExpr) -> Result<RequirementSource, String> {
    let fields = fields(map.syntax(), "lockfile requirement source")?;
    if fields.iter().any(|field| field.name == "simi") {
        return Err(
            "lockfile pins the runtime-owned `std` catalog; remove the source `std` requirement and regenerate the lockfile"
                .to_owned(),
        );
    }
    reject_unknown(
        &fields,
        &["git", "rev", "path"],
        "lockfile requirement source",
    )?;
    let git = optional_string(&fields, "git", "lockfile requirement source")?;
    let rev = optional_string(&fields, "rev", "lockfile requirement source")?;
    let path = optional_string(&fields, "path", "lockfile requirement source")?;
    match (git, rev, path) {
        (Some(git), Some(rev), None) if !git.is_empty() && !rev.is_empty() => {
            Ok(RequirementSource::Git { git, rev })
        }
        (None, None, Some(path)) if valid_relative_path(&path) => Ok(RequirementSource::Path { path }),
        _ => Err("lockfile requirement source must be `{git = string, rev = string}` or `{path = string}`".to_owned()),
    }
}

fn expression(statement: &ExprStmt) -> Result<SyntaxExpr, String> {
    ast::child(statement.syntax()).ok_or_else(|| "lockfile expression is missing".to_owned())
}

fn fields(node: &simi_syntax::SyntaxNode, context: &str) -> Result<Vec<Field>, String> {
    let mut names = BTreeMap::new();
    let mut result = Vec::new();
    for entry in node.children().filter_map(MapEntry::cast) {
        let expressions = entry
            .syntax()
            .children()
            .filter_map(SyntaxExpr::cast)
            .collect::<Vec<_>>();
        let (name, value) = if let Some(token) = ast::token(entry.syntax(), SyntaxKind::IDENT) {
            let Some(value) = expressions.into_iter().next() else {
                return Err(format!(
                    "{context} field `{}` requires a value",
                    token.text()
                ));
            };
            (token.text().to_owned(), value)
        } else if ast::token(entry.syntax(), SyntaxKind::L_BRACKET).is_some() {
            let [key, value] = expressions.as_slice() else {
                return Err(format!("{context} computed keys require a value"));
            };
            (
                static_string(key, &format!("{context} computed key"))?,
                value.clone(),
            )
        } else {
            return Err(format!(
                "{context} keys must be identifiers or string literals"
            ));
        };
        if names.insert(name.clone(), ()).is_some() {
            return Err(format!("{context} declares field `{name}` more than once"));
        }
        result.push(Field { name, value });
    }
    Ok(result)
}

fn reject_unknown(fields: &[Field], known: &[&str], context: &str) -> Result<(), String> {
    if let Some(field) = fields
        .iter()
        .find(|field| !known.contains(&field.name.as_str()))
    {
        return Err(format!("{context} does not permit field `{}`", field.name));
    }
    Ok(())
}

fn required_map(fields: &[Field], name: &str, context: &str) -> Result<MapExpr, String> {
    let field = fields
        .iter()
        .find(|field| field.name == name)
        .ok_or_else(|| format!("{context} requires field `{name}`"))?;
    match field.value.clone() {
        SyntaxExpr::Map(map) => Ok(map),
        _ => Err(format!("{context} field `{name}` must be a map")),
    }
}

fn required_string(fields: &[Field], name: &str, context: &str) -> Result<String, String> {
    let field = fields
        .iter()
        .find(|field| field.name == name)
        .ok_or_else(|| format!("{context} requires field `{name}`"))?;
    static_string(&field.value, &format!("{context} field `{name}`"))
}

fn optional_string(fields: &[Field], name: &str, context: &str) -> Result<Option<String>, String> {
    fields
        .iter()
        .find(|field| field.name == name)
        .map(|field| static_string(&field.value, &format!("{context} field `{name}`")))
        .transpose()
}

fn required_integer(fields: &[Field], name: &str, context: &str) -> Result<u64, String> {
    let field = fields
        .iter()
        .find(|field| field.name == name)
        .ok_or_else(|| format!("{context} requires field `{name}`"))?;
    let SyntaxExpr::Literal(literal) = &field.value else {
        return Err(format!("{context} field `{name}` must be an integer"));
    };
    let Some(token) = ast::token(literal.syntax(), SyntaxKind::INT) else {
        return Err(format!("{context} field `{name}` must be an integer"));
    };
    token
        .text()
        .parse()
        .map_err(|_| format!("{context} field `{name}` must be an integer"))
}

fn static_string(expression: &SyntaxExpr, context: &str) -> Result<String, String> {
    let SyntaxExpr::Literal(literal) = expression else {
        return Err(format!("{context} must be a string literal"));
    };
    let Some(token) = ast::token(literal.syntax(), SyntaxKind::STRING) else {
        return Err(format!("{context} must be a string literal"));
    };
    decode_string(token.text()).ok_or_else(|| format!("{context} has an invalid string literal"))
}

fn decode_string(text: &str) -> Option<String> {
    let body = text.strip_prefix('"')?.strip_suffix('"')?;
    let mut output = String::new();
    let mut chars = body.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        output.push(match chars.next()? {
            '"' => '"',
            '\\' => '\\',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            _ => return None,
        });
    }
    Some(output)
}

fn render_requirement_source(output: &mut String, source: &RequirementSource) {
    match source {
        RequirementSource::Git { git, rev } => {
            output.push_str("{git = ");
            output.push_str(&string(git));
            output.push_str(", rev = ");
            output.push_str(&string(rev));
            output.push('}');
        }
        RequirementSource::Path { path } => {
            output.push_str("{path = ");
            output.push_str(&string(path));
            output.push('}');
        }
    }
}

fn render_map_key(output: &mut String, name: &str) {
    if valid_identifier(name) {
        output.push_str(name);
    } else {
        output.push('[');
        output.push_str(&string(name));
        output.push(']');
    }
}

fn valid_identifier(name: &str) -> bool {
    let mut characters = name.chars();
    matches!(characters.next(), Some('a'..='z' | 'A'..='Z' | '_'))
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn string(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            _ => output.push(character),
        }
    }
    output.push('"');
    output
}

fn valid_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
}
