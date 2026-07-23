use std::collections::HashMap;

use simi_analysis::{AnalysisDatabase, Type, infer_types, parse, resolve, symbol_type_at};

fn inferred(
    source: &str,
) -> (
    simi_analysis::TypeInference,
    std::sync::Arc<simi_analysis::Resolution>,
) {
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    assert!(
        parse(&db, file).diagnostics.is_empty(),
        "syntax diagnostics: {:?}",
        parse(&db, file).diagnostics
    );
    let resolution = resolve(&db, file);
    (infer_types(&db, file, &HashMap::new()), resolution)
}

#[test]
fn fibonacci_example_is_syntax_and_type_clean() {
    let db = AnalysisDatabase::default();
    let modules = [
        ("std/number", include_str!("../../../stdlib/number.simi")),
        ("std/io", include_str!("../../../stdlib/io.simi")),
    ]
    .into_iter()
    .map(|(name, source)| {
        let file = db.add_file(source);
        (name.to_owned(), simi_analysis::module_shape(&db, file))
    })
    .collect::<HashMap<_, _>>();
    let source = include_str!("../../../examples/fibonacci.simi");
    let file = db.add_file(source);
    assert!(
        parse(&db, file).diagnostics.is_empty(),
        "syntax diagnostics: {:?}",
        parse(&db, file).diagnostics
    );
    let inference = infer_types(&db, file, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "type diagnostics: {:?}",
        inference.diagnostics
    );
}

fn type_of(
    inference: &simi_analysis::TypeInference,
    resolution: &simi_analysis::Resolution,
    name: &str,
) -> Type {
    let symbol = resolution
        .hir
        .symbols
        .iter()
        .find(|(_, symbol)| symbol.name == name && !symbol.builtin)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("missing symbol {name}"));
    inference.symbol_types[&symbol].clone()
}

fn nth_offset(source: &str, needle: &str, occurrence: usize) -> usize {
    source
        .match_indices(needle)
        .nth(occurrence)
        .unwrap_or_else(|| panic!("missing occurrence {occurrence} of {needle}"))
        .0
}

fn type_at(
    source: &str,
    inference: &simi_analysis::TypeInference,
    resolution: &simi_analysis::Resolution,
    needle: &str,
    occurrence: usize,
) -> String {
    symbol_type_at(
        inference,
        resolution,
        nth_offset(source, needle, occurrence),
    )
    .unwrap_or_else(|| panic!("missing type at occurrence {occurrence} of {needle}"))
    .display()
}

#[test]
fn operators_annotations_generics_and_literals_infer_stable_types() {
    let source = r#"
fn process(n) do n + 1 end
fn increment(n: integer) do n + 1 end
fn identity(value) do value end
fn mixed_generics(explicit: 'a, inferred) do inferred end
fn choose(flag, value) do if flag then value else nil end end
let selected = identity("text")
let integer = 1 + 2
let mixed = 1 + 2.0
let quotient = 1 / 2
let values = [1, "two"]
let record = { name = "Simi", age = 1 }
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "process").display(),
        "(integer | float) -> integer | float"
    );
    assert_eq!(
        type_of(&inference, &resolution, "increment").display(),
        "integer -> integer"
    );
    assert_eq!(
        type_of(&inference, &resolution, "identity").display(),
        "'a -> 'a"
    );
    assert_eq!(
        type_of(&inference, &resolution, "mixed_generics").display(),
        "('a, 'b) -> 'b"
    );
    assert_eq!(
        type_of(&inference, &resolution, "choose").display(),
        "(boolean, 'a) -> 'a | nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "selected").display(),
        "\"text\""
    );
    assert_eq!(
        type_of(&inference, &resolution, "integer").display(),
        "integer"
    );
    assert_eq!(type_of(&inference, &resolution, "mixed").display(), "float");
    assert_eq!(
        type_of(&inference, &resolution, "quotient").display(),
        "float"
    );
    assert_eq!(
        type_of(&inference, &resolution, "values").display(),
        "[integer, \"two\"]"
    );
    assert_eq!(
        type_of(&inference, &resolution, "record").display(),
        "{ name: \"Simi\", age: integer }"
    );
}

#[test]
fn loop_state_transitions_and_break_values_infer_the_loop_result() {
    let source = r#"
fn fib(n: integer) do
    loop state = { a = 0, b = 1, n = n } do
        case state.n
        of 0 do
            break state.a
        of _ do
            { a = state.b, b = state.a + state.b, n = state.n - 1 }
        end
    end
end
let result = fib(5)
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "fib").display(),
        "integer -> integer"
    );
    assert_eq!(
        type_of(&inference, &resolution, "result").display(),
        "integer"
    );
}

#[test]
fn pipelines_and_trailing_arguments_use_call_inference() {
    let source = r#"
fn combine(value: integer, suffix: string) -> string do suffix end
let piped = 1 |> combine("x")
let trailing = combine(1) <| "x"
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "piped").display(),
        "string"
    );
    assert_eq!(
        type_of(&inference, &resolution, "trailing").display(),
        "string"
    );
}

#[test]
fn aliases_and_function_types_are_transparent_and_right_associative() {
    let source = r#"
alias option<'a> = 'a | nil
let callback: integer -> string | nil = fn(value: integer) -> string | nil do
    if value == 0 then nil else "value" end
end
let result: option<string> = callback(1)
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "callback").display(),
        "integer -> string | nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "result").display(),
        "string | nil"
    );
}

#[test]
fn loop_breaks_keep_path_values_and_nonreturning_loops_are_never() {
    let source = r#"
let value = 1
let selected = loop state = 0 do
    if state == 0 then
        break value
    else
        value = "later"
        state + 1
    end
end
let joined = loop state = {common=1, left=2} do
    if state.common == 1 then
        {common=2, right="new"}
    else
        break state
    end
end
fn nested(flag) do
    loop state = [] do
        if flag then [state] else break state end
    end
end
let forever = loop state = 0 do state + 1 end
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "selected").display(),
        "integer"
    );
    assert_eq!(
        type_of(&inference, &resolution, "forever").display(),
        "never"
    );
    assert_eq!(
        type_of(&inference, &resolution, "joined").display(),
        "{ common: integer, .. }"
    );
    assert_eq!(
        type_of(&inference, &resolution, "nested").display(),
        "boolean -> [..any]"
    );
}

#[test]
fn map_index_signatures_type_dynamic_reads_and_reject_wrong_keys() {
    let db = AnalysisDatabase::default();
    let file = db.add_file(concat!(
        "let values: { [string]: integer } = { answer = 42 }\n",
        "let key = \"answer\"\n",
        "let found = values[key]\n",
        "let bad = values[1]\n",
    ));
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &HashMap::new());
    assert_eq!(
        type_of(&inference, &resolution, "found").display(),
        "integer | nil"
    );
    assert_eq!(
        inference
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        vec!["type_mismatch"]
    );
}

#[test]
fn empty_lists_start_with_an_exact_empty_shape() {
    let (inference, resolution) = inferred("let empty = []");
    assert!(inference.diagnostics.is_empty());
    assert_eq!(type_of(&inference, &resolution, "empty").display(), "[]");
}

#[test]
fn known_list_append_refines_empty_lists_and_all_aliases() {
    let db = AnalysisDatabase::default();
    let module_file =
        db.add_file("fn append(xs: [..'a], x: 'a) -> nil do nil end { append = append }");
    let modules = HashMap::from([(
        "std/list".to_owned(),
        simi_analysis::module_shape(&db, module_file),
    )]);
    let file = db.add_file(
        "let list = require(\"std/list\") let values = [] let alias = values list.append(values, 1)",
    );
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "values").display(),
        "[integer]"
    );
    assert_eq!(
        type_of(&inference, &resolution, "alias").display(),
        "[integer]"
    );
}

#[test]
fn declared_post_types_update_all_aliases_after_normal_calls() {
    let source = r#"
fn widen(xs: [..integer]) -> nil
    after xs becomes [..integer | string]
do host.call("opaque/widen", xs) end
let values = [1]
let alias = values
widen(values)
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "values").display(),
        "[..integer | string]"
    );
    assert_eq!(
        type_of(&inference, &resolution, "alias").display(),
        "[..integer | string]"
    );
    let widen = type_of(&inference, &resolution, "widen");
    assert_eq!(widen.display(), "[..integer] -> nil");
}

#[test]
fn named_wrappers_infer_and_propagate_parameter_post_states() {
    let db = AnalysisDatabase::default();
    let module_file = db.add_file(include_str!("../../../stdlib/list.simi"));
    let modules = HashMap::from([(
        "std/list".to_owned(),
        simi_analysis::module_shape(&db, module_file),
    )]);
    let source = r#"
let list = require("std/list")
fn append(xs, value) do list.append(xs, value) end
fn append_then_break(xs, value) do
    loop state = nil do
        list.append(xs, value)
        break nil
    end
end
let append_alias = append
let values = []
let alias = values
append_alias(values, 1)
let stopped = []
append_then_break(stopped, 2)
let piped = [1] |> tap append_alias(2) |> tap append_alias(3)
"#;
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    let append = resolution
        .hir
        .symbols
        .iter()
        .find(|(_, symbol)| symbol.name == "append" && !symbol.builtin)
        .map(|(symbol, _)| symbol)
        .expect("append symbol");
    assert_eq!(
        inference.symbol_types[&append].display(),
        "([..'a], 'b) -> nil"
    );
    assert_eq!(inference.symbol_posts[&append].len(), 1);
    assert_eq!(
        type_of(&inference, &resolution, "append_alias").display(),
        "([..'a], 'b) -> nil"
    );
    assert_eq!(inference.symbol_posts[&append][0].parameter_name, "xs");
    assert_eq!(
        inference.symbol_posts[&append][0].becomes.display(),
        "[..'a | 'b]"
    );
    assert_eq!(
        type_of(&inference, &resolution, "values").display(),
        "[..integer]"
    );
    assert_eq!(
        type_of(&inference, &resolution, "alias").display(),
        "[..integer]"
    );
    assert_eq!(
        type_of(&inference, &resolution, "stopped").display(),
        "[..integer]"
    );
    let append_then_break = resolution
        .hir
        .symbols
        .iter()
        .find(|(_, symbol)| symbol.name == "append_then_break" && !symbol.builtin)
        .map(|(symbol, _)| symbol)
        .expect("append_then_break symbol");
    assert_eq!(inference.symbol_posts[&append_then_break].len(), 1);
    assert_eq!(
        type_of(&inference, &resolution, "piped").display(),
        "[..integer]"
    );
}

#[test]
fn copied_list_mutation_does_not_infer_an_input_post_state() {
    let db = AnalysisDatabase::default();
    let module_file = db.add_file(include_str!("../../../stdlib/list.simi"));
    let modules = HashMap::from([(
        "std/list".to_owned(),
        simi_analysis::module_shape(&db, module_file),
    )]);
    let source = r#"
let list = require("std/list")
let ns = [1]
fn immut_append(ns, n) do
    ns |> list.copy() |> tap list.append(n)
end
let ns_new = ns |> immut_append(2) |> immut_append(3)
ns
"#;
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "ns").display(),
        "[integer]"
    );
    assert_eq!(
        type_of(&inference, &resolution, "ns_new").display(),
        "[..integer]"
    );
    let immut_append = resolution
        .hir
        .symbols
        .iter()
        .find(|(_, symbol)| symbol.name == "immut_append" && !symbol.builtin)
        .map(|(symbol, _)| symbol)
        .expect("immut_append symbol");
    assert!(
        inference
            .symbol_posts
            .get(&immut_append)
            .is_none_or(Vec::is_empty)
    );
}

#[test]
fn source_posts_are_verified_and_host_facades_are_trusted() {
    let source = r#"
fn wrong(xs: [..integer]) -> nil
    after xs becomes [..string]
do nil end
fn trusted(xs: [..integer]) -> nil
    after xs becomes [..string]
do host.call("opaque", xs) end
fn invalid(value: integer) -> nil
    after value becomes string
do nil end
fn unknown(value: integer) -> nil
    after missing becomes integer
do nil end
"#;
    let (inference, _) = inferred(source);
    let titles = inference
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.title.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        titles,
        vec![
            "Unknown post-type parameter",
            "Post-type is not established",
            "Invalid post-type",
        ]
    );
}

#[test]
fn shadow_versions_keep_distinct_symbol_and_closure_types() {
    let source = r#"let value = 1
let before = fn() do value end
let value = "new"
let after_value = fn() do value end"#;
    let (inference, resolution) = inferred(source);
    assert!(inference.diagnostics.is_empty());
    let mut values = resolution
        .hir
        .symbols
        .iter()
        .filter(|(_, symbol)| symbol.name == "value")
        .collect::<Vec<_>>();
    values.sort_by_key(|(_, symbol)| symbol.declaration.unwrap().start);
    assert_eq!(inference.symbol_types[&values[0].0].display(), "integer");
    assert_eq!(inference.symbol_types[&values[1].0].display(), "\"new\"");
    assert_eq!(
        type_of(&inference, &resolution, "before").display(),
        "() -> integer"
    );
    assert_eq!(
        type_of(&inference, &resolution, "after_value").display(),
        "() -> \"new\""
    );
}

#[test]
fn cycle_pipeline_preserves_precise_mutated_shape_across_same_scope_shadow() {
    let db = AnalysisDatabase::default();
    let module_file = db.add_file(include_str!("../../../stdlib/list.simi"));
    let modules = HashMap::from([(
        "std/list".to_owned(),
        simi_analysis::module_shape(&db, module_file),
    )]);
    let source = r#"let list = require("std/list")
let nums = [1, 2, 3]
let nums = nums |> tap list.append(nums)
nums[3]"#;
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );

    let mut nums = resolution
        .hir
        .symbols
        .iter()
        .filter(|(_, symbol)| symbol.name == "nums")
        .collect::<Vec<_>>();
    nums.sort_by_key(|(_, symbol)| symbol.declaration.unwrap().start);
    assert_eq!(nums.len(), 2);
    let expected = "[integer, integer, integer, [integer, integer, integer]]";
    assert_eq!(inference.symbol_types[&nums[0].0].display(), expected);
    assert_eq!(inference.symbol_types[&nums[1].0].display(), expected);

    let rhs_start = source.find("nums |> tap").unwrap();
    let append_argument = source.rfind("nums)").unwrap();
    let final_read = source.rfind("nums[3]").unwrap();
    assert_eq!(resolution.symbol_at(rhs_start), Some(nums[0].0));
    assert_eq!(resolution.symbol_at(append_argument), Some(nums[0].0));
    assert_eq!(resolution.symbol_at(final_read), Some(nums[1].0));
    let final_type = inference
        .expression_types
        .iter()
        .find(|(span, _)| span.start == final_read && span.end == source.len())
        .map(|(_, ty)| ty.display());
    assert_eq!(final_type.as_deref(), Some("[integer, integer, integer]"));
}

#[test]
fn annotated_generic_stdlib_calls_infer_through_nested_type_variables() {
    let db = AnalysisDatabase::default();
    let list_file = db.add_file(include_str!("../../../stdlib/list.simi"));
    let iter_file = db.add_file(include_str!("../../../stdlib/iter.simi"));
    let modules = HashMap::from([
        (
            "std/list".to_owned(),
            simi_analysis::module_shape(&db, list_file),
        ),
        (
            "std/iter".to_owned(),
            simi_analysis::module_shape(&db, iter_file),
        ),
    ]);
    let file = db.add_file(concat!(
        "let list = require(\"std/list\")\n",
        "let iter = require(\"std/iter\")\n",
        "let mapped = iter.to_list(iter.map(list.iter([1, 2]), fn(value) do value + 1 end))\n",
        "let found = iter.find(list.iter([1, 2]), fn(value) do value > 1 end)\n",
    ));
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "mapped").display(),
        "[..any]"
    );
    assert_eq!(type_of(&inference, &resolution, "found").display(), "'a");
}

#[test]
fn malformed_alias_uses_produce_bounded_diagnostics() {
    let source = r#"
alias option<'a> = 'a | nil
alias recursive = recursive
let unknown: missing = 1
let wrong: option<integer, string> = 1
let cycle: recursive = 1
"#;
    let (inference, _) = inferred(source);
    let codes = inference
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"unknown_type"));
    assert!(codes.contains(&"wrong_type_arity"));
    assert!(codes.contains(&"cyclic_type_alias"));
    assert!(inference.diagnostics.len() < 10);
}

#[test]
fn definite_type_errors_have_stable_codes() {
    let source = r#"
let declared: integer = "wrong"
let bad_operator = "x" + true
let not_callable = 1(2)
fn one(value: integer) -> integer do value end
let bad_argument = one("x")
one()
"#;
    let (inference, _) = inferred(source);
    let codes = inference
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"type_mismatch"));
    assert_eq!(
        codes
            .iter()
            .filter(|code| **code == "invalid_operator")
            .count(),
        1
    );
    assert!(codes.contains(&"not_callable"));
    assert!(codes.contains(&"wrong_arity"));
}

#[test]
fn conditions_narrow_builtin_categories_nil_literals_and_discriminants() {
    let source = r#"
alias result = { kind: "ok", value: integer } | { kind: "error", error: string }
fn classify(value: integer | string | nil) do
    if type(value) == "integer" then
        value
    elseif value == nil then
        "nil"
    else
        value
    end
end
fn read(item: result) do
    if item.kind == "ok" then
        item.value
    else
        item.error
    end
end
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "value", 3),
        "integer"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "value", 5),
        "string"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "item", 2),
        "{ kind: \"ok\", value: integer }"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "item", 3),
        "{ kind: \"error\", error: string }"
    );
}

#[test]
fn short_circuit_guards_narrow_rhs_and_join_assignments() {
    let source = r#"
fn choose(input: string | nil) do
    if nil != input and (input == "x" or input == "y") then
        input
    else
        "other"
    end
end
fn replace(flag: boolean) do
    let value = 1
    if flag then value = "new" else nil end
    value
end
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "input", 2),
        "string"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "input", 4),
        "\"x\" | \"y\""
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "value", 2),
        "integer | \"new\""
    );
}

#[test]
fn shadowed_type_does_not_narrow_and_boolean_complements_are_local() {
    let source = r#"
fn type(ignored) do "integer" end
fn inspect(subject: integer | string) do
    if not (type(subject) != "integer") then subject else subject end
end
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "subject", 2),
        "integer | string"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "subject", 3),
        "integer | string"
    );
}

#[test]
fn structural_mutation_invalidates_discriminant_facts() {
    let source = r#"
alias outcome = { kind: "ok", value: integer } | { kind: "error", error: string }
fn mutate(item: outcome) do
    if item.kind == "ok" then
        item.kind = "ok"
        item
    else
        item
    end
end
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "item", 3),
        "{ kind: \"ok\", value: integer }"
    );
}

#[test]
fn case_patterns_narrow_structural_union_and_bind_payloads() {
    let source = r#"
alias result = { kind: "ok", value: integer } | { kind: "error", error: string }
fn unwrap(result: result) do
    case result
    of { kind = "ok", value = payload } do payload
    of { kind = "error", error = message } do message
    end
end
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "payload", 1),
        "integer"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "message", 1),
        "string"
    );
}

#[test]
fn nil_propagation_stops_at_every_lexical_block_during_inference() {
    let source = r#"
fn named(value: integer | nil) do
    value?
    1
end
let anonymous = fn(value: integer | nil) do
    value?
    1
end
fn boundaries(value: integer | nil) do
    let standalone = do value? 1 end
    let selected_if = if true then value? 1 else 2 end
    let selected_else = if false then 1 else value? 2 end
    let selected_case = case 1 of 1 do value? 1 end
    let protected = try value? 1 catch _ do 2 end
    let caught = try raise "failure" catch _ do value? 1 end
    [standalone, selected_if, selected_else, selected_case, protected, caught]
    "continued"
end
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "named").display(),
        "(integer | nil) -> integer | nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "anonymous").display(),
        "(integer | nil) -> integer | nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "boundaries").display(),
        "(integer | nil) -> \"continued\""
    );
}

#[test]
fn nil_propagation_contributes_nil_to_the_loop_state_fixed_point() {
    let source = r#"
fn evolve(maybe: integer | nil) do
    loop state = 0 do
        if state == nil then
            break state
        end
        maybe?
        state + 1
    end
end
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "state", 1),
        "integer | nil"
    );
    assert_eq!(type_at(source, &inference, &resolution, "state", 2), "nil");
    assert_eq!(
        type_at(source, &inference, &resolution, "state", 3),
        "integer"
    );
    assert_eq!(
        type_of(&inference, &resolution, "evolve").display(),
        "(integer | nil) -> nil"
    );
}

#[test]
fn nil_propagation_narrows_only_the_normal_block_continuation() {
    let source = r#"
fn unwrap(value: string | nil) do
    let result = do
        value?
        value
    end
    value
    result
end
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "value", 1),
        "string | nil"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "value", 2),
        "string"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "value", 3),
        "string | nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "unwrap").display(),
        "(string | nil) -> string | nil"
    );
}

#[test]
fn mutation_hovers_use_the_type_at_each_source_occurrence() {
    let db = AnalysisDatabase::default();
    let module_file = db.add_file(include_str!("../../../stdlib/list.simi"));
    let modules = HashMap::from([(
        "std/list".to_owned(),
        simi_analysis::module_shape(&db, module_file),
    )]);
    let source = r#"let list = require("std/list")
let ns = [1, 2]
ns
list.append(ns, 3)
ns"#;
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "ns", 0),
        "[integer, integer]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "ns", 1),
        "[integer, integer]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "ns", 2),
        "[integer, integer]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "ns", 3),
        "[integer, integer, integer]"
    );
}

#[test]
fn closure_bodies_do_not_mutate_declaration_time_outer_flow() {
    let db = AnalysisDatabase::default();
    let module_file = db.add_file(include_str!("../../../stdlib/list.simi"));
    let modules = HashMap::from([(
        "std/list".to_owned(),
        simi_analysis::module_shape(&db, module_file),
    )]);
    let source = r#"
let list = require("std/list")
let named = [1, 2]
fn later() do list.append(named, 3) end
named
let anonymous = [1, 2]
let callback = fn() do list.append(anonymous, 3) end
anonymous
"#;
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "named", 2),
        "[integer, integer]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "anonymous", 2),
        "[integer, integer]"
    );
}

#[test]
fn analyzed_calls_preserve_arguments_while_unknown_calls_widen() {
    let db = AnalysisDatabase::default();
    let module_file = db.add_file(include_str!("../../../stdlib/list.simi"));
    let modules = HashMap::from([(
        "std/list".to_owned(),
        simi_analysis::module_shape(&db, module_file),
    )]);
    let source = r#"
let list = require("std/list")
fn opaque(value) do value end
let first = [1, 2]
opaque(first)
first
let callable: any = opaque
let second = [1, 2]
callable(second)
second
let precise = [1, 2]
list.append(precise, 3)
precise
"#;
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "first", 2),
        "[integer, integer]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "second", 2),
        "[integer, integer]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "precise", 2),
        "[integer, integer, integer]"
    );
}

#[test]
fn unmodeled_calls_follow_any_alias_regions_and_analyzed_callbacks() {
    let db = AnalysisDatabase::default();
    let list_file = db.add_file(include_str!("../../../stdlib/list.simi"));
    let iter_file = db.add_file(include_str!("../../../stdlib/iter.simi"));
    let modules = HashMap::from([
        (
            "std/list".to_owned(),
            simi_analysis::module_shape(&db, list_file),
        ),
        (
            "std/iter".to_owned(),
            simi_analysis::module_shape(&db, iter_file),
        ),
    ]);
    let source = r#"
let list = require("std/list")
let iter = require("std/iter")
fn mutate(value: any) do value end
let values = [1, 2]
let hidden: any = values
mutate(hidden)
values
fn visit(value: integer) -> any do value end
let callback_values = [1, 2]
iter.each(list.iter(callback_values), visit)
callback_values
"#;
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "values", 2),
        "[integer, integer]"
    );
    assert_eq!(type_at(source, &inference, &resolution, "hidden", 1), "any");
    assert_eq!(
        type_at(source, &inference, &resolution, "callback_values", 2),
        "[..any]"
    );
}

#[test]
fn map_writes_update_aliases_without_restoring_stale_discriminants() {
    let source = r#"
let record = {kind = "ok", payload = 1}
let mirror = record
record.kind = "error"
mirror
record.kind = nil
mirror
let indexed = {kind = "ok"}
indexed["kind"] = "error"
indexed
let dynamic = {kind = "ok"}
let key = "kind"
dynamic[key] = nil
dynamic
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "mirror", 1),
        "{ kind: \"error\", payload: integer }"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "mirror", 2),
        "{ payload: integer }"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "indexed", 2),
        "{ kind: \"error\" }"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "dynamic", 2),
        "{ .. }"
    );
}

#[test]
fn structural_patterns_keep_heterogeneous_rest_and_require_closed_map_fields() {
    let source = r#"
let values: [..integer | string] = [1, "two"]
let tail = case values
of [1, ..rest] do rest
of _ do []
end
let closed = {present = 1}
let result = case closed
of {missing = missing} do missing
of _ do "fallback"
end
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "rest").display(),
        "[..integer | string]"
    );
    assert_eq!(
        type_of(&inference, &resolution, "tail").display(),
        "[..integer | string]"
    );
    assert_eq!(
        type_of(&inference, &resolution, "result").display(),
        "\"fallback\""
    );
}

#[test]
fn map_patterns_respect_optional_presence_and_all_required_fields() {
    let source = r#"
let absent = {missing = nil}
let absent_binding = case absent
of {missing = value} do "present"
of _ do "absent"
end
let absent_nil = case absent
of {missing = nil} do "nil"
of _ do "other"
end
fn maybe(value: string | nil) do
    let record = {maybe = value}
    case record
    of {maybe = present} do "present"
    of _ do "absent"
    end
end
fn indexed(record: {[string]: integer}) do
    case record
    of {missing = value} do "present"
    of _ do "absent"
    end
end
fn opened(record: {..}) do
    case record
    of {missing = value} do "present"
    of _ do "absent"
    end
end
fn multiple(record: {first: "yes", second: "ok" | "no"}) do
    case record
    of {first = "yes", second = "ok"} do "matched"
    of _ do "fallback"
    end
end
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert!(matches!(
        type_of(&inference, &resolution, "absent"),
        Type::Map {
            ref fields,
            index: None,
            open: false,
        } if fields.is_empty()
    ));
    assert_eq!(
        type_of(&inference, &resolution, "absent_binding").display(),
        "\"absent\""
    );
    assert_eq!(
        type_of(&inference, &resolution, "absent_nil").display(),
        "\"nil\""
    );
    for name in ["maybe", "indexed", "opened"] {
        assert_eq!(
            type_of(&inference, &resolution, name).display(),
            match name {
                "maybe" => "(string | nil) -> \"present\" | \"absent\"",
                "indexed" => "{ [string]: integer } -> \"present\" | \"absent\"",
                _ => "{ .. } -> \"present\" | \"absent\"",
            }
        );
    }
    assert_eq!(
        type_of(&inference, &resolution, "multiple").display(),
        "{ first: \"yes\", second: \"ok\" | \"no\" } -> \"matched\" | \"fallback\""
    );
}

#[test]
fn closure_calls_and_callbacks_invalidate_captured_mutable_regions() {
    let db = AnalysisDatabase::default();
    let list_file = db.add_file(include_str!("../../../stdlib/list.simi"));
    let iter_file = db.add_file(include_str!("../../../stdlib/iter.simi"));
    let modules = HashMap::from([
        (
            "std/list".to_owned(),
            simi_analysis::module_shape(&db, list_file),
        ),
        (
            "std/iter".to_owned(),
            simi_analysis::module_shape(&db, iter_file),
        ),
    ]);
    let source = r#"
let list = require("std/list")
let iter = require("std/iter")
let named_values = [1]
fn mutate_named() do list.append(named_values, 2) end
named_values
mutate_named()
named_values
let anonymous_values = [1]
let mutate_anonymous = fn() do list.append(anonymous_values, 2) end
anonymous_values
mutate_anonymous()
anonymous_values
let visited = [1]
let separate = [2]
fn visit(value: integer) -> nil do list.append(separate, 3) end
iter.each(list.iter(visited), visit)
separate
let accumulator = [0]
fn keep(acc: [integer], value: integer) -> [integer] do acc end
iter.fold(list.iter([1]), accumulator, keep)
accumulator
fn mutate_forward() do list.append(forward_values, 2) end
let forward_values = [1]
mutate_forward()
forward_values
let nested_list_outer = {inner = [1]}
let nested_list = nested_list_outer.inner
list.append(nested_list, 2)
nested_list_outer
"#;
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "named_values", 2),
        "[integer]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "named_values", 3),
        "[..any]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "anonymous_values", 2),
        "[integer]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "anonymous_values", 3),
        "[..any]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "separate", 2),
        "[..any]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "accumulator", 2),
        "[integer]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "forward_values", 2),
        "[..any]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "nested_list_outer", 2),
        "{ .. }"
    );
}

#[test]
fn unknown_callable_invocations_widen_all_mutable_regions_only_on_active_paths() {
    let db = AnalysisDatabase::default();
    let module_file = db.add_file(include_str!("../../../stdlib/list.simi"));
    let modules = HashMap::from([(
        "std/list".to_owned(),
        simi_analysis::module_shape(&db, module_file),
    )]);
    let source = r#"
let list = require("std/list")
let values = [1]
let handlers = {
    run = fn() do list.append(values, 2) end,
}
let invoke: any = handlers.run
invoke()
values
let returned_values = [1]
fn make_handler() do
    fn() do list.append(returned_values, 2) end
end
let returned = make_handler()
returned_values
returned()
returned_values
let pipeline_values = [1]
let pipeline_handlers = {
    run = fn(incoming) do list.append(pipeline_values, incoming) end,
}
let pipeline_invoke: any = pipeline_handlers.run
nil ?> pipeline_invoke()
pipeline_values
1 ?> pipeline_invoke()
pipeline_values
"#;
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "values", 2),
        "[..any]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "returned_values", 2),
        "[integer]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "returned_values", 3),
        "[..any]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "pipeline_values", 2),
        "[integer]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "pipeline_values", 3),
        "[..any]"
    );
}

#[test]
fn callable_reassignment_clears_posts_and_builtin_trust() {
    let db = AnalysisDatabase::default();
    let module_file = db.add_file(include_str!("../../../stdlib/list.simi"));
    let modules = HashMap::from([(
        "std/list".to_owned(),
        simi_analysis::module_shape(&db, module_file),
    )]);
    let source = r#"
let list = require("std/list")
let values = [1]
let handlers = {
    run = fn() do list.append(values, 2) end,
}
fn invoke() do nil end
invoke = handlers.run
invoke()
values
let builtin_values = [1]
let builtin_handlers = {
    run = fn() do list.append(builtin_values, 2) end,
}
type = builtin_handlers.run
type()
builtin_values
let candidate: integer | string = 1
type = fn(value) do "integer" end
let selected = if type(candidate) == "integer" then candidate else nil end
"#;
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "values", 2),
        "[..any]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "builtin_values", 2),
        "[..any]"
    );
    assert_eq!(
        type_of(&inference, &resolution, "selected").display(),
        "integer | string | nil"
    );
}

#[test]
fn callable_metadata_joins_and_invoked_assignments_are_conservative() {
    let db = AnalysisDatabase::default();
    let module_file = db.add_file(include_str!("../../../stdlib/list.simi"));
    let modules = HashMap::from([(
        "std/list".to_owned(),
        simi_analysis::module_shape(&db, module_file),
    )]);
    let source = r#"
let list = require("std/list")
fn widen(xs: [..integer]) -> nil
    after xs becomes [..integer | string]
do host.call("test/widen", xs) end
fn plain(xs: [..integer]) do nil end
let flag: boolean = true
let selected = widen
if flag then selected = plain end
let post_values = [1]
selected(post_values)
post_values
let left = [1]
let right = [1]
let callback = fn() do list.append(left, 2) end
if flag then callback = fn() do list.append(right, 2) end end
callback()
left
right
let builtin_values = [1]
let handlers = { run = fn() do list.append(builtin_values, 2) end }
fn replace() do type = handlers.run end
fn wrapper() do replace() end
wrapper()
type()
builtin_values
let candidate: integer | string = 1
let narrowed = if type(candidate) == "integer" then candidate else nil end
"#;
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "post_values", 2),
        "[..any]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "left", 2),
        "[..any]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "right", 2),
        "[..any]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "builtin_values", 2),
        "[..any]"
    );
    assert_eq!(
        type_of(&inference, &resolution, "narrowed").display(),
        "integer | string | nil"
    );
}

#[test]
fn callable_assignment_effects_propagate_through_multiple_wrappers() {
    let db = AnalysisDatabase::default();
    let module_file = db.add_file(include_str!("../../../stdlib/list.simi"));
    let modules = HashMap::from([(
        "std/list".to_owned(),
        simi_analysis::module_shape(&db, module_file),
    )]);
    let source = r#"
let list = require("std/list")
let values = [1]
let handlers = { run = fn() do list.append(values, 2) end }
fn replace() do type = handlers.run end
fn wrapper() do replace() end
fn outer_wrapper() do wrapper() end
outer_wrapper()
type()
values
let candidate: integer | string = 1
let narrowed = if type(candidate) == "integer" then candidate else nil end
"#;
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "values", 2),
        "[..any]"
    );
    assert_eq!(
        type_of(&inference, &resolution, "narrowed").display(),
        "integer | string | nil"
    );
}

#[test]
fn unannotated_case_patterns_seed_body_stable_list_and_map_domains() {
    let source = r#"
fn first_or_nil(values) do
    case values
    of [value, ..rest] do value
    of [] do nil
    end
end
fn read_value(record) do
    case record
    of {value=value} do value
    end
end
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "first_or_nil").display(),
        "[..'a] -> 'a | nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "read_value").display(),
        "{ value: 'a, .. } -> 'a"
    );
}

#[test]
fn recursive_result_inference_is_occurs_safe_and_uses_returning_evidence() {
    let source = r#"
fn forever() do forever() end
fn eventually(flag) do if flag then 1 else eventually(flag) end end
fn left() do right() end
fn right() do left() end
fn nested() do [nested()] end
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "forever").display(),
        "() -> never"
    );
    assert_eq!(
        type_of(&inference, &resolution, "eventually").display(),
        "boolean -> integer"
    );
    assert_eq!(
        type_of(&inference, &resolution, "left").display(),
        "() -> never"
    );
    assert_eq!(
        type_of(&inference, &resolution, "right").display(),
        "() -> never"
    );
    assert_eq!(
        type_of(&inference, &resolution, "nested").display(),
        "() -> never"
    );
}

#[test]
fn mixed_structural_cases_keep_possible_clauses_and_fallbacks() {
    let source = r#"
fn list_or_nil(value) do
    case value
    of [] do 1
    of _ do nil
    end
end
fn map_or_nil(value) do
    case value
    of {item=_} do 1
    of fallback do nil
    end
end
fn literal_list_or_nil(value) do
    case value
    of "skip" do "text"
    of [] do 1
    of _ do nil
    end
end
fn alternative_maps(value) do
    case value
    of {left=_} do 1
    of {right=_} do "right"
    end
end
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "list_or_nil").display(),
        "'a -> integer | nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "map_or_nil").display(),
        "'a -> integer | nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "literal_list_or_nil").display(),
        "'a -> integer | \"text\" | nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "alternative_maps").display(),
        "({ left: 'a, .. } | { right: 'b, .. }) -> integer | \"right\""
    );
}

#[test]
fn fully_unannotated_recursive_quicksort_has_a_reachable_list_signature() {
    let db = AnalysisDatabase::default();
    let module_file = db.add_file(include_str!("../../../stdlib/list.simi"));
    let modules = HashMap::from([(
        "std/list".to_owned(),
        simi_analysis::module_shape(&db, module_file),
    )]);
    let source = r#"
let list = require("std/list")
fn partition(values, pivot) do
    loop state = {remaining=values, lower=[], higher=[]} do
        case state
        of {remaining=[], lower=lower, higher=higher} do
            break {lower=lower, higher=higher}
        of {remaining=[value, ..rest], lower=lower, higher=higher} when value < pivot do
            {remaining=rest, lower=lower |> tap list.append(value), higher=higher}
        of {remaining=[value, ..rest], lower=lower, higher=higher} do
            {remaining=rest, lower=lower, higher=higher |> tap list.append(value)}
        end
    end
end
fn quicksort(values) do
    case values
    of [] do []
    of [value] do [value]
    of [pivot, ..rest] do
        let parts = partition(rest, pivot)
        []
        |> tap list.extend(quicksort(parts.lower))
        |> tap list.append(pivot)
        |> tap list.extend(quicksort(parts.higher))
    end
end
"#;
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "quicksort").display(),
        "[..integer | float] -> [..integer | float]"
    );
}

#[test]
fn append_driven_loop_state_infers_an_integer_list_result() {
    let db = AnalysisDatabase::default();
    let module_file = db.add_file(include_str!("../../../stdlib/list.simi"));
    let modules = HashMap::from([(
        "std/list".to_owned(),
        simi_analysis::module_shape(&db, module_file),
    )]);
    let source = r#"
let list = require("std/list")
fn sum_list(ns: [..integer]) do
    loop state = {acc=0, ns=ns} do
        case state.ns
        of [] do break state.acc
        of [head, ..tail] do {acc=state.acc + head, ns=tail}
        end
    end
end
let ns = loop state = {acc=[], i=0} do
    if state.i < 1000 then
        {acc=state.acc |> tap list.append(state.i), i=state.i + 1}
    else
        break state.acc
    end
end
sum_list(ns)
"#;
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "ns").display(),
        "[..integer]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "ns", 5),
        "[..integer]"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "ns", 6),
        "[..integer]"
    );
}

#[test]
fn nested_read_alias_mutations_invalidate_roots_and_outer_aliases() {
    let source = r#"
let outer = {inner = {kind = "ok"}}
let outer_alias = outer
let inner = outer.inner
inner.kind = "error"
outer
outer_alias
let indexed_outer = {inner = {kind = "ok"}}
let indexed_alias = indexed_outer
let indexed_inner = indexed_outer["inner"]
indexed_inner["kind"] = nil
indexed_outer
indexed_alias
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    for (name, occurrence) in [
        ("outer", 4),
        ("outer_alias", 1),
        ("indexed_outer", 3),
        ("indexed_alias", 1),
    ] {
        assert_eq!(
            type_at(source, &inference, &resolution, name, occurrence),
            "{ .. }"
        );
    }
}

#[test]
fn nested_mutations_invalidate_root_aliases() {
    let source = r#"
let outer = {inner = {kind = "ok"}}
let alias = outer
outer.inner.kind = "error"
alias
let indexed = {items = [{kind = "ok"}]}
let indexed_alias = indexed
indexed.items[0].kind = nil
indexed_alias
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "alias", 1),
        "{ .. }"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "indexed_alias", 1),
        "{ .. }"
    );
}

#[test]
fn temporal_any_and_constant_boolean_reachability_are_preserved() {
    let source = r#"
let value: any = 1
value
value = "later"
value
let selected = if true then 1 else "unreachable" end
let short = false and ("bad" + true)
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(type_at(source, &inference, &resolution, "value", 1), "any");
    assert_eq!(type_at(source, &inference, &resolution, "value", 2), "any");
    assert_eq!(
        type_at(source, &inference, &resolution, "value", 3),
        "\"later\""
    );
    assert_eq!(
        type_of(&inference, &resolution, "selected").display(),
        "integer"
    );
    assert_eq!(
        type_of(&inference, &resolution, "short").display(),
        "boolean"
    );
}

#[test]
fn nil_aware_pipeline_splits_effects_and_bottom_is_normalized() {
    let db = AnalysisDatabase::default();
    let module_file = db.add_file(include_str!("../../../stdlib/list.simi"));
    let modules = HashMap::from([(
        "std/list".to_owned(),
        simi_analysis::module_shape(&db, module_file),
    )]);
    let source = r#"
let list = require("std/list")
fn append_if_present(values: [integer, integer] | nil) do
    values ?> tap list.append(3)
    values
end
fn ignored(value: any, extra: any) do value end
fn kind(value: any) -> string do type(value) end
let mixed = nil ?> ignored("x" + true) |> kind()
"#;
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "values", 2),
        "[integer, integer, integer] | nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "mixed").display(),
        "string"
    );
}
