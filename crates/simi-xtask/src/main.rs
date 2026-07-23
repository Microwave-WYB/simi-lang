use std::{
    env, fs,
    io::Write,
    path::PathBuf,
    process::{self, Command, Stdio},
};

const TOKENS: &[&str] = &[
    "ERROR_TOKEN",
    "WHITESPACE",
    "COMMENT",
    "INT",
    "FLOAT",
    "STRING",
    "IDENT",
    "FN_KW",
    "DO_KW",
    "END_KW",
    "IF_KW",
    "THEN_KW",
    "AFTER_KW",
    "ELSEIF_KW",
    "ELSE_KW",
    "LET_KW",
    "ALIAS_KW",
    "BECOMES_KW",
    "TAP_KW",
    "NIL_KW",
    "TRUE_KW",
    "FALSE_KW",
    "AND_KW",
    "OR_KW",
    "NOT_KW",
    "LOOP_KW",
    "BREAK_KW",
    "CONTINUE_KW",
    "CASE_KW",
    "OF_KW",
    "WHEN_KW",
    "RAISE_KW",
    "TRY_KW",
    "CATCH_KW",
    "L_PAREN",
    "R_PAREN",
    "L_BRACKET",
    "R_BRACKET",
    "L_BRACE",
    "R_BRACE",
    "COMMA",
    "COLON",
    "APOSTROPHE",
    "ARROW",
    "PIPE",
    "DOT",
    "DOT_DOT",
    "EQ",
    "EQ_EQ",
    "BANG_EQ",
    "PLUS",
    "MINUS",
    "STAR",
    "SLASH",
    "SLASH_SLASH",
    "PERCENT",
    "LESS",
    "LESS_EQ",
    "GREATER",
    "GREATER_EQ",
    "QUESTION",
    "QUESTION_GREATER",
    "PIPE_GREATER",
    "LESS_PIPE",
];
fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let grammar_path = root.join("crates/simi-syntax/simi.ungram");
    let generated_path = root.join("crates/simi-syntax/src/generated.rs");
    let grammar_text = fs::read_to_string(&grammar_path).expect("read simi.ungram");
    let grammar: ungrammar::Grammar = grammar_text.parse().expect("parse simi.ungram");
    let output = format_generated(&generate(&grammar));
    match env::args().nth(1).as_deref() {
        Some("codegen") => {
            fs::write(&generated_path, output).expect("write generated.rs");
            println!("generated syntax kinds and wrappers from simi.ungram");
        }
        None | Some("check") => {
            let current = fs::read_to_string(&generated_path).expect("read generated.rs");
            if current != output {
                eprintln!("generated syntax is stale; run `cargo run -p simi-xtask -- codegen`");
                process::exit(1);
            }
            println!("generated syntax is current");
        }
        Some(command) => {
            eprintln!("unknown command `{command}`; expected `check` or `codegen`");
            process::exit(2);
        }
    }
}

fn format_generated(source: &str) -> String {
    let mut child = Command::new("rustfmt")
        .args(["--edition", "2024", "--emit", "stdout"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("run rustfmt for generated syntax");
    child
        .stdin
        .as_mut()
        .expect("rustfmt stdin")
        .write_all(source.as_bytes())
        .expect("write generated syntax to rustfmt");
    let output = child.wait_with_output().expect("wait for rustfmt");
    if !output.status.success() {
        panic!("rustfmt rejected generated syntax");
    }
    String::from_utf8(output.stdout).expect("rustfmt output is UTF-8")
}

fn generate(grammar: &ungrammar::Grammar) -> String {
    let all = grammar
        .iter()
        .map(|node| grammar[node].name.clone())
        .collect::<Vec<_>>();
    let concrete = all
        .iter()
        .filter(|name| !matches!(name.as_str(), "Stmt" | "Expr" | "Pattern"))
        .cloned()
        .collect::<Vec<_>>();
    let mut out = String::from(
        "// Generated from simi.ungram by `cargo run -p simi-xtask -- codegen`.\n// Do not edit by hand.\n\nuse crate::syntax::SyntaxNode;\n\n#[allow(non_camel_case_types)]\n#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]\n#[repr(u16)]\npub enum SyntaxKind {\n",
    );
    for token in TOKENS {
        out.push_str(&format!("    {token},\n"));
    }
    for node in &concrete {
        out.push_str(&format!("    {},\n", shout(node)));
    }
    out.push_str("}\n\npub trait AstNode: Sized {\n    fn can_cast(kind: SyntaxKind) -> bool;\n    fn cast(syntax: SyntaxNode) -> Option<Self>;\n    fn syntax(&self) -> &SyntaxNode;\n}\n\nmacro_rules! ast_node {\n    ($name:ident, $kind:ident) => {\n        #[derive(Clone, Debug, PartialEq, Eq)]\n        pub struct $name { syntax: SyntaxNode }\n        impl AstNode for $name {\n            fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::$kind }\n            fn cast(syntax: SyntaxNode) -> Option<Self> { Self::can_cast(syntax.kind()).then_some(Self { syntax }) }\n            fn syntax(&self) -> &SyntaxNode { &self.syntax }\n        }\n    };\n}\n\n");
    for node in &concrete {
        out.push_str(&format!("ast_node!({node}, {});\n", shout(node)));
    }
    out.push_str("\n#[derive(Clone, Debug, PartialEq, Eq)]\npub enum Stmt { FunctionDecl(FunctionDecl), AliasDecl(AliasDecl), LetStmt(LetStmt), ExprStmt(ExprStmt) }\nimpl AstNode for Stmt {\n    fn can_cast(kind: SyntaxKind) -> bool { matches!(kind, SyntaxKind::FUNCTION_DECL | SyntaxKind::ALIAS_DECL | SyntaxKind::LET_STMT | SyntaxKind::EXPR_STMT) }\n    fn cast(syntax: SyntaxNode) -> Option<Self> { Some(match syntax.kind() { SyntaxKind::FUNCTION_DECL => Self::FunctionDecl(FunctionDecl::cast(syntax)?), SyntaxKind::ALIAS_DECL => Self::AliasDecl(AliasDecl::cast(syntax)?), SyntaxKind::LET_STMT => Self::LetStmt(LetStmt::cast(syntax)?), SyntaxKind::EXPR_STMT => Self::ExprStmt(ExprStmt::cast(syntax)?), _ => return None }) }\n    fn syntax(&self) -> &SyntaxNode { match self { Self::FunctionDecl(node) => node.syntax(), Self::AliasDecl(node) => node.syntax(), Self::LetStmt(node) => node.syntax(), Self::ExprStmt(node) => node.syntax() } }\n}\n\n");
    let expressions = union_nodes(grammar, "Expr");
    let patterns = union_nodes(grammar, "Pattern");
    enum_code(&mut out, "Expr", &expressions, "kind.is_expression()");
    enum_code(&mut out, "Pattern", &patterns, "kind.is_pattern()");
    out.push_str("impl Root { pub fn statements(&self) -> impl Iterator<Item = Stmt> + '_ { crate::ast::children(self.syntax()) } }\nimpl Block { pub fn statements(&self) -> impl Iterator<Item = Stmt> + '_ { crate::ast::children(self.syntax()) } }\n");
    out
}

fn union_nodes(grammar: &ungrammar::Grammar, name: &str) -> Vec<String> {
    let node = grammar
        .iter()
        .find(|node| grammar[*node].name == name)
        .unwrap_or_else(|| panic!("missing `{name}` union in ungrammar"));
    let ungrammar::Rule::Alt(alternatives) = &grammar[node].rule else {
        panic!("`{name}` must be an alternative union");
    };
    alternatives
        .iter()
        .map(|rule| {
            let ungrammar::Rule::Node(node) = rule else {
                panic!("`{name}` union members must be nodes");
            };
            grammar[*node].name.clone()
        })
        .collect()
}

fn enum_code(out: &mut String, name: &str, nodes: &[String], can_cast: &str) {
    out.push_str(&format!(
        "#[derive(Clone, Debug, PartialEq, Eq)]\npub enum {name} {{\n"
    ));
    for node in nodes {
        out.push_str(&format!("    {}({node}),\n", node.trim_end_matches(name)));
    }
    out.push_str("}\n");
    out.push_str(&format!("impl AstNode for {name} {{\n    fn can_cast(kind: SyntaxKind) -> bool {{ {can_cast} }}\n    fn cast(syntax: SyntaxNode) -> Option<Self> {{ Some(match syntax.kind() {{\n"));
    for node in nodes {
        out.push_str(&format!(
            "        SyntaxKind::{} => Self::{}({node}::cast(syntax)?),\n",
            shout(node),
            node.trim_end_matches(name)
        ));
    }
    out.push_str(
        "        _ => return None,\n    }) }\n    fn syntax(&self) -> &SyntaxNode { match self {\n",
    );
    for node in nodes {
        out.push_str(&format!(
            "        Self::{}(node) => node.syntax(),\n",
            node.trim_end_matches(name)
        ));
    }
    out.push_str("    } }\n}\n\n");
}

fn shout(name: &str) -> String {
    let mut result = String::new();
    for (index, ch) in name.chars().enumerate() {
        if ch.is_uppercase() && index != 0 {
            result.push('_');
        }
        result.extend(ch.to_uppercase());
    }
    result
}
