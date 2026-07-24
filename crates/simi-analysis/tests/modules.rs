use std::collections::HashMap;

use simi_analysis::{
    AnalysisDatabase, CallableType, Type, imported_members, imported_modules, member_at,
    member_completions, module_at, module_shape,
};

#[test]
fn documented_typed_native_aliases_keep_callable_module_metadata() {
    let source = r#"
--- Return the text length.
let length: string -> integer = host.length
--- Append a value.
let append: ([..'a] => [..('a | 'b)], 'b) -> nil = host.append
{length = length, append = append}
"#;
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let shape = module_shape(&db, file);
    let length = &shape.fields[0];
    assert_eq!(length.name, "length");
    assert_eq!(
        length.documentation.as_deref(),
        Some("Return the text length.")
    );
    assert_eq!(
        length.ty,
        Some(Type::Function(Box::new(CallableType::inferred(
            vec![Type::String],
            Type::Int,
            Type::Any,
        ))))
    );
    assert!(length.parameters.is_none());

    let append = &shape.fields[1];
    assert_eq!(append.name, "append");
    assert_eq!(append.posts.len(), 1);
    assert_eq!(append.posts[0].parameter_index, 0);
    assert_eq!(append.posts[0].becomes.display(), "[..('a | 'b)]");
}

#[test]
fn function_type_aliases_preserve_post_state_metadata() {
    let source = r#"
alias appender<'a, 'b> = ([..'a] => [..('a | 'b)], 'b) -> nil
let append: appender<integer, string> = host.append
let wrapped: ((appender<integer, string>)) = host.append
{append = append, wrapped = wrapped}
"#;
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let shape = module_shape(&db, file);
    for field in &shape.fields {
        assert_eq!(field.posts.len(), 1);
        assert_eq!(field.posts[0].becomes.display(), "[..(integer | string)]");
    }
}

#[test]
fn documented_discard_bindings_do_not_overwrite_prior_symbol_docs() {
    let source = r#"
--- Prior documentation.
let prior = 1
--- Discard documentation.
let _ignored = 2
{prior = prior}
"#;
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let shape = module_shape(&db, file);
    assert_eq!(
        shape.fields[0].documentation.as_deref(),
        Some("Prior documentation.")
    );
}

#[test]
fn infers_exported_functions_parameters_docs_and_nested_maps() {
    let source = r#"
--- Append one value.
fn append(xs, x) do nil end
fn hidden(value) do value end
{
    append = append,
    nested = { hidden = hidden },
}
"#;
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let shape = module_shape(&db, file);
    assert_eq!(shape.fields.len(), 2);
    let append = &shape.fields[0];
    assert_eq!(append.name, "append");
    assert_eq!(
        append.parameters.as_deref(),
        Some(&["xs".to_owned(), "x".to_owned()][..])
    );
    assert_eq!(append.documentation.as_deref(), Some("Append one value."));
    assert_eq!(shape.fields[1].fields[0].name, "hidden");

    let consumer = "let module = require(\"nested\") module.nested.";
    let consumer_file = db.add_file(consumer);
    let modules = HashMap::from([("nested".to_owned(), shape)]);
    let completions = member_completions(&db, consumer_file, &modules, consumer, consumer.len());
    assert_eq!(completions.len(), 1);
    assert_eq!(completions[0].name, "hidden");
}

#[test]
fn literal_unshadowed_require_provides_members_but_shadowed_require_does_not() {
    let module_source = "fn append(xs, x) do nil end { append = append }";
    let source = "let list = require(\"std/list\") list.append";
    let db = AnalysisDatabase::default();
    let module_file = db.add_file(module_source);
    let file = db.add_file(source);
    let modules = HashMap::from([("std/list".to_owned(), module_shape(&db, module_file))]);
    let offset = source.len();
    assert_eq!(imported_modules(&db, file).len(), 1);
    let member = member_at(&db, file, &modules, source, offset - 1).expect("known member");
    assert_eq!(member.field.name, "append");
    assert_eq!(member.field.parameters.as_ref().unwrap(), &["xs", "x"]);

    let incomplete = "let list = require(\"std/list\") list.";
    let incomplete_file = db.add_file(incomplete);
    let completions =
        member_completions(&db, incomplete_file, &modules, incomplete, incomplete.len());
    assert_eq!(completions.len(), 1);

    let shadowed = "let require = fn(name) do nil end let list = require(\"std/list\") list.";
    let shadowed_file = db.add_file(shadowed);
    assert!(member_completions(&db, shadowed_file, &modules, shadowed, shadowed.len()).is_empty());
}

#[test]
fn propagates_direct_module_fields_through_bindings() {
    let module_source = r#"
--- Print one value.
fn println(value) do nil end
{ println = println }
"#;
    let db = AnalysisDatabase::default();
    let module_file = db.add_file(module_source);
    let modules = HashMap::from([("std/io".to_owned(), module_shape(&db, module_file))]);

    let direct = "require(\"std/io\").println";
    let direct_file = db.add_file(direct);
    let direct_member = member_at(
        &db,
        direct_file,
        &modules,
        direct,
        direct.rfind("println").unwrap(),
    )
    .expect("direct require field");
    assert_eq!(
        direct_member.field.parameters.as_deref(),
        Some(&["value".to_owned()][..])
    );
    assert_eq!(
        direct_member.field.documentation.as_deref(),
        Some("Print one value.")
    );

    let aliased = "let print = require(\"std/io\").println print";
    let aliased_file = db.add_file(aliased);
    let alias_offset = aliased.rfind("print").unwrap();
    let alias = member_at(&db, aliased_file, &modules, aliased, alias_offset)
        .expect("aliased module field");
    assert_eq!(alias.field.name, "print");
    assert_eq!(
        alias.field.parameters.as_deref(),
        Some(&["value".to_owned()][..])
    );
    assert_eq!(
        alias.field.documentation.as_deref(),
        Some("Print one value.")
    );
    assert_eq!(imported_members(&db, aliased_file, &modules).len(), 1);

    let incomplete = "require(\"std/io\").";
    let incomplete_file = db.add_file(incomplete);
    let completions =
        member_completions(&db, incomplete_file, &modules, incomplete, incomplete.len());
    assert_eq!(completions.len(), 1);
    assert_eq!(completions[0].name, "println");
}

#[test]
fn infers_simple_mutable_export_map() {
    let source = r#"
fn run(value) do value end
let exports = {}
exports.run = run
exports
"#;
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let shape = module_shape(&db, file);
    assert_eq!(shape.fields.len(), 1);
    assert_eq!(shape.fields[0].name, "run");
    assert_eq!(shape.fields[0].parameters.as_ref().unwrap(), &["value"]);
}

#[test]
fn only_the_final_module_value_defines_the_export_shape() {
    let db = AnalysisDatabase::default();

    for source in [
        "{ stale = 1 } nil",
        "{ stale = 1 } fn final_declaration() do nil end",
        "let exports = { stale = 1 } exports exports.stale = nil",
    ] {
        let file = db.add_file(source);
        assert!(
            module_shape(&db, file).fields.is_empty(),
            "advertised stale exports for {source:?}"
        );
    }
}

#[test]
fn module_documentation_is_distinct_and_follows_module_values() {
    let module_source = r#"
---- Standard output operations.
---- Values are flushed automatically.

--- Print one value.
fn println(value) do nil end
{ println = println }
"#;
    let db = AnalysisDatabase::default();
    let module_file = db.add_file(module_source);
    let shape = module_shape(&db, module_file);
    assert_eq!(
        shape.documentation.as_deref(),
        Some("Standard output operations.\nValues are flushed automatically.")
    );
    assert_eq!(
        shape.fields[0].documentation.as_deref(),
        Some("Print one value.")
    );

    let source = "let stdout = require(\"std/io\") stdout";
    let file = db.add_file(source);
    let modules = HashMap::from([("std/io".to_owned(), shape)]);
    for offset in [
        source.find("\"std/io\"").unwrap() + 1,
        source.rfind("stdout").unwrap(),
    ] {
        let module = module_at(&db, file, &modules, offset).expect("known module value");
        assert_eq!(module.module, "std/io");
        assert_eq!(
            module.documentation.as_deref(),
            Some("Standard output operations.\nValues are flushed automatically.")
        );
    }
}

#[test]
fn annotated_exported_functions_carry_types_and_trailing_aliases_are_erased() {
    let source = r#"
fn map(xs: [..'a], transform: 'a -> 'b) -> [..'b] do [] end
{ map = map, identity = fn(value) do value end }
alias option<'a> = 'a | nil
"#;
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let shape = module_shape(&db, file);
    let map = shape
        .fields
        .iter()
        .find(|field| field.name == "map")
        .unwrap();
    assert_eq!(
        map.ty.as_ref().map(simi_analysis::Type::display).as_deref(),
        Some("(xs: [..'a], transform: 'a -> 'b) -> [..'b]")
    );
    let identity = shape
        .fields
        .iter()
        .find(|field| field.name == "identity")
        .unwrap();
    assert_eq!(
        identity
            .ty
            .as_ref()
            .map(simi_analysis::Type::display)
            .as_deref(),
        Some("(value: 'a) -> 'a")
    );
}

#[test]
fn documentation_requires_immediately_consecutive_triple_dash_comments() {
    let source = r#"
--- Attached line one.
--- Attached line two.
fn attached(value) do value end

--- Separated.

fn blank(value) do value end

--- Interrupted.
-- Ordinary comment.
fn ordinary(value) do value end

{ attached = attached, blank = blank, ordinary = ordinary }
"#;
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let shape = module_shape(&db, file);
    assert_eq!(
        shape.fields[0].documentation.as_deref(),
        Some("Attached line one.\nAttached line two.")
    );
    assert_eq!(shape.fields[1].documentation, None);
    assert_eq!(shape.fields[2].documentation, None);
}

#[test]
fn imports_are_scope_aware_exact_calls_and_nil_fields_are_deleted() {
    let module_source = "fn run(value) do value end { run = run }";
    let consumer = r#"
fn nested() do
    let module = require("known")
    module.
end
let dynamic = "known"
let wrong = require(dynamic)
let extra = require("known", "ignored")
"#;
    let db = AnalysisDatabase::default();
    let module_file = db.add_file(module_source);
    let file = db.add_file(consumer);
    let modules = HashMap::from([("known".to_owned(), module_shape(&db, module_file))]);
    let member_offset = consumer.find("module.").unwrap() + "module.".len();
    assert_eq!(
        member_completions(&db, file, &modules, consumer, member_offset).len(),
        1
    );
    assert_eq!(simi_analysis::imported_modules(&db, file).len(), 1);

    let deleted = r#"
fn kept(value) do value end
let exports = { omitted = nil, kept = kept }
exports.kept = nil
exports.added = kept
exports
"#;
    let deleted_file = db.add_file(deleted);
    let shape = module_shape(&db, deleted_file);
    assert_eq!(shape.fields.len(), 1);
    assert_eq!(shape.fields[0].name, "added");
}
