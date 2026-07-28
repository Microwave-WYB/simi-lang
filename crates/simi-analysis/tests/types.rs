use std::collections::HashMap;

use simi_analysis::{
    AnalysisDatabase, AnalysisDiagnosticCode, AnalysisDiagnosticSeverity, Type, diagnostics,
    expression_type_at, infer_types, module_shape, parse, resolve, symbol_type_at,
};

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
fn radix_and_separator_literals_have_numeric_static_categories() {
    let source = r#"
let binary = 0b_1010
let hexadecimal = 0x_ff
let integer = 1_000
let decimal = 1_000.25
let exponent = 1.5_0e1_0
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    for name in ["binary", "hexadecimal", "integer"] {
        assert_eq!(type_of(&inference, &resolution, name).display(), "integer");
    }
    for name in ["decimal", "exponent"] {
        assert_eq!(type_of(&inference, &resolution, name).display(), "float");
    }
}

#[test]
fn built_in_number_alias_accepts_both_numeric_categories_without_runtime_meaning() {
    let source = r#"
let whole: number = 1
let fractional: number = 1.5
let mismatch: number = "text"
"#;
    let (inference, resolution) = inferred(source);
    let number = Type::Union(vec![Type::Int, Type::Float]);
    assert_eq!(type_of(&inference, &resolution, "whole"), number);
    assert_eq!(
        type_of(&inference, &resolution, "fractional"),
        Type::Union(vec![Type::Int, Type::Float])
    );
    assert!(
        inference
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == AnalysisDiagnosticCode::TypeMismatch)
    );
}

#[test]
fn boolean_singletons_are_narrow_record_discriminants() {
    let source = r#"
alias Step<'a> =
    | {done: true, ..}
    | {done: false, value: 'a, ..}
alias EitherBoolean = true | false
let flag = false
let exhausted = {done = true}
let yielded = {done = false, value = 1}
let either: EitherBoolean = true
fn next<'a>(item: 'a, stop: boolean) -> Step<'a> do
    if stop then {done = true} else {done = false, value = item} end
end
fn read(step: Step<integer>) -> integer | nil do
    if step.done then
        let exhausted_value = step.value
        exhausted_value
    else
        let payload = step.value
        payload
    end
end
let nil_item: Step<integer | nil> = {done = false}
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(type_of(&inference, &resolution, "flag"), Type::Boolean);
    assert_eq!(
        type_of(&inference, &resolution, "exhausted").display(),
        "{ done: true }"
    );
    assert_eq!(
        type_of(&inference, &resolution, "yielded").display(),
        "{ done: false, value: integer }"
    );
    assert_eq!(type_of(&inference, &resolution, "either"), Type::Boolean);
    assert_eq!(
        type_of(&inference, &resolution, "exhausted_value"),
        Type::Any
    );
    assert_eq!(type_of(&inference, &resolution, "payload"), Type::Int);
}

#[test]
fn explicit_primitive_singletons_are_erased_and_expressions_stay_wide() {
    let source = r#"
alias Scalar = nil | true | false | "ready" | 42 | 1.0 | -0.0
let count = 42
let ratio = 1.0
let enabled = true
let exact_nil: nil = nil
let exact_true: true = true
let exact_false: false = false
let exact_text: "ready" = "ready"
let exact_integer: 42 = 42
let exact_hex: 0x2a = 42
let exact_float: 1.0 = 1.0
let exact_exponent: 1e3 = 1000.0
let normalized_zero: 0.0 = -0.0
fn accept(value: 42) -> integer do value end
fn exact_result() -> 42 do 42 end
let accepted = accept(42)
let returned = exact_result()
let wrong_integer: 42 = 43
let wrong_category: 1.0 = 1
let computed: 42 = 40 + 2
"#;
    let (inference, resolution) = inferred(source);
    assert_eq!(type_of(&inference, &resolution, "count"), Type::Int);
    assert_eq!(type_of(&inference, &resolution, "ratio"), Type::Float);
    assert_eq!(type_of(&inference, &resolution, "enabled"), Type::Boolean);
    assert_eq!(type_of(&inference, &resolution, "exact_nil"), Type::Nil);
    assert_eq!(
        type_of(&inference, &resolution, "exact_true"),
        Type::LiteralBoolean(true)
    );
    assert_eq!(
        type_of(&inference, &resolution, "exact_false"),
        Type::LiteralBoolean(false)
    );
    assert_eq!(
        type_of(&inference, &resolution, "exact_text"),
        Type::LiteralString("ready".to_owned())
    );
    assert_eq!(
        type_of(&inference, &resolution, "exact_integer"),
        Type::LiteralInt(42)
    );
    assert_eq!(
        type_of(&inference, &resolution, "exact_hex"),
        Type::LiteralInt(42)
    );
    assert_eq!(
        type_of(&inference, &resolution, "exact_float").display(),
        "1.0"
    );
    assert_eq!(
        type_of(&inference, &resolution, "exact_exponent").display(),
        "1000.0"
    );
    assert_eq!(
        type_of(&inference, &resolution, "normalized_zero").display(),
        "0.0"
    );
    assert_eq!(type_of(&inference, &resolution, "accepted"), Type::Int);
    assert_eq!(
        type_of(&inference, &resolution, "returned"),
        Type::LiteralInt(42)
    );
    assert_eq!(
        inference
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == AnalysisDiagnosticCode::TypeMismatch)
            .count(),
        3,
        "{:?}",
        inference.diagnostics
    );
}

#[test]
fn singleton_context_applies_to_function_bodies_and_mutation_rhs() {
    let source = r#"
fn direct_true() -> true true
fn direct_int() -> 42 42
let anon = fn() -> false false
let annotated: () -> true noraise = fn() true
let tagged: {done: true} = {done = true}
tagged.done = true
let indexed: {done: true} = {done = true}
indexed["done"] = true
let initial_code: 41 = 41
let field_union: {code: 41 | 42} = {code = initial_code}
field_union.code = 42
field_union.code = 41
let index_union: {code: 41 | 42} = {code = initial_code}
index_union["code"] = 42
index_union["code"] = 41
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "direct_true").display(),
        "() -> true"
    );
    assert_eq!(
        type_of(&inference, &resolution, "direct_int").display(),
        "() -> 42"
    );
    assert_eq!(
        type_of(&inference, &resolution, "anon").display(),
        "() -> false"
    );
    assert_eq!(
        type_of(&inference, &resolution, "annotated").display(),
        "() -> true noraise"
    );
    for name in ["tagged", "indexed"] {
        assert_eq!(
            type_of(&inference, &resolution, name).display(),
            "{ done: true }"
        );
    }
    for name in ["field_union", "index_union"] {
        assert_eq!(
            type_of(&inference, &resolution, name).display(),
            "{ code: 41 | 42 }"
        );
    }
}

#[test]
fn contextual_callable_let_annotations_keep_effect_and_explicit_result_checks() {
    let source = r#"
let inferred: (value: true) -> true noraise = fn(value) value
let raised: () -> never raises string = fn() raise "failure"
let explicit_result_mismatch: () -> true noraise = fn() -> false noraise false
let effect_mismatch: () -> true noraise = fn() raise "failure"
"#;
    let (inference, resolution) = inferred(source);
    assert_eq!(
        type_of(&inference, &resolution, "inferred").display(),
        "(value: true) -> true noraise"
    );
    assert_eq!(
        type_of(&inference, &resolution, "raised").display(),
        "() -> never raises string"
    );
    assert_eq!(
        inference
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == AnalysisDiagnosticCode::TypeMismatch)
            .count(),
        2,
        "{:?}",
        inference.diagnostics
    );
}

#[test]
fn broad_values_do_not_satisfy_singleton_mutation_targets() {
    let source = r#"
let field_flag: {done: true} = {done = true}
let index_flag: {done: true} = {done = true}
let broad_flag = false and true
field_flag.done = broad_flag
index_flag["done"] = broad_flag
let exact_number: 42 = 42
let field_number: {code: 42} = {code = exact_number}
let index_number: {code: 42} = {code = exact_number}
let broad_number = 40 + 2
field_number.code = broad_number
index_number["code"] = broad_number
"#;
    let (inference, _) = inferred(source);
    assert_eq!(
        inference
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == AnalysisDiagnosticCode::TypeMismatch)
            .count(),
        4,
        "{:?}",
        inference.diagnostics
    );
}

#[test]
fn arbitrary_booleans_do_not_satisfy_singleton_fields() {
    let source = r#"
alias Step<'a> =
    | {done: true, ..}
    | {done: false, value: 'a, ..}
let missing: Step<integer> = {done = false}
let broad: true = false and true
let computed = {done = false and true}
let computed_step: Step<integer> = computed
"#;
    let (inference, resolution) = inferred(source);
    assert_eq!(
        type_of(&inference, &resolution, "broad"),
        Type::LiteralBoolean(true)
    );
    assert_eq!(
        type_of(&inference, &resolution, "computed").display(),
        "{ done: boolean }"
    );
    assert_eq!(
        inference
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == AnalysisDiagnosticCode::TypeMismatch)
            .count(),
        3,
        "{:?}",
        inference.diagnostics
    );
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
let empty_record = {}
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
        "(n: integer | float) -> integer | float"
    );
    assert_eq!(
        type_of(&inference, &resolution, "increment").display(),
        "(n: integer) -> integer"
    );
    assert_eq!(
        type_of(&inference, &resolution, "identity").display(),
        "(value: 'a) -> 'a"
    );
    assert_eq!(
        type_of(&inference, &resolution, "mixed_generics").display(),
        "(explicit: 'a, inferred: 'b) -> 'b"
    );
    assert_eq!(
        type_of(&inference, &resolution, "choose").display(),
        "(flag: boolean, value: 'a) -> 'a | nil"
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
        type_of(&inference, &resolution, "empty_record").display(),
        "{}"
    );
    assert_eq!(
        type_of(&inference, &resolution, "record").display(),
        "{ name: \"Simi\", age: integer }"
    );
}

#[test]
fn literal_require_calls_use_the_evaluated_module_result_type() {
    let db = AnalysisDatabase::default();
    let module_file = db.add_file("let exports = { answer = 42, empty = {} } exports");
    let shape = module_shape(&db, module_file);
    assert_eq!(
        shape.ty.as_ref().map(Type::display).as_deref(),
        Some("{ answer: integer, empty: {} }")
    );

    let source = "let data = require(\"known\")\ndata";
    let file = db.add_file(source);
    let modules = HashMap::from([("known".to_owned(), shape)]);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "data").display(),
        "{ answer: integer, empty: {} }"
    );
    assert_eq!(
        expression_type_at(
            &inference,
            source.find(')').expect("require closing delimiter")
        )
        .map(|(_, ty)| ty.display())
        .as_deref(),
        Some("{ answer: integer, empty: {} }")
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
    let file = db.add_file(" let values = [] let alias = values list.append(values, 1)");
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
    let source = r#"
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
    let list_shape = simi_analysis::module_shape(&db, list_file);
    let iter_shape = simi_analysis::module_shape(&db, iter_file);
    let modules = HashMap::from([
        ("std/list".to_owned(), list_shape),
        ("std/iter".to_owned(), iter_shape),
    ]);
    let file = db.add_file(concat!(
        "\n",
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
        "[..integer]"
    );
    assert_eq!(
        type_of(&inference, &resolution, "found").display(),
        "integer | nil"
    );
}

#[test]
fn iterator_items_contextualize_callbacks_across_call_forms() {
    let db = AnalysisDatabase::default();
    let modules = [
        ("std/list", include_str!("../../../stdlib/list.simi")),
        ("std/map", include_str!("../../../stdlib/map.simi")),
        ("std/iter", include_str!("../../../stdlib/iter.simi")),
    ]
    .into_iter()
    .map(|(name, source)| {
        let file = db.add_file(source);
        (name.to_owned(), simi_analysis::module_shape(&db, file))
    })
    .collect::<HashMap<_, _>>();
    let source = r#"
let iter = require("std/iter")
let folded = iter.fold(list.iter([1, 2, 3]), 0, fn(acc, fold_item) do acc + fold_item end)
let piped =
    [1, 2, 3]
    |> list.iter()
    |> iter.map(fn(pipeline_item) do pipeline_item + 1 end)
    |> iter.to_list()
let trailing_iterator =
    iter.map(list.iter([1, 2, 3])) <| fn(trailing_item) do trailing_item + 1 end
let trailing = iter.to_list(trailing_iterator)
let mixed = iter.fold(list.iter([1, 2.0]), 0.0, fn(mixed_acc, mixed_item) do
    mixed_acc + mixed_item
end)
let mapped = iter.to_list(iter.map(list.iter([1, 2]), fn(map_item) do map_item + 1 end))
let filtered = iter.to_list(iter.filter(list.iter([1, 2]), fn(filter_item) do filter_item > 1 end))
let found = iter.find(list.iter([1, 2]), fn(find_item) do find_item > 1 end)
let nil_items = iter.to_list(iter.map(list.iter([1, nil, 3]), fn(nil_item) do nil_item end))
let keys =
    map.iter({first = 1})
    |> iter.map(fn(entry) do entry.key end)
    |> iter.to_list()
let map_step = iter.next(map.iter({}))
if map_step.done then
    let exhausted_entry = map_step.value
else
    let live_entry = map_step.value
end
fn transform<'a, 'b, 'e>(value: 'a, callback: 'a -> 'b raises 'e) -> 'b raises 'e do
    callback(value)
end
let generic_result = transform(1, fn(generic_item) do generic_item + 1 end)
let parenthesized = transform(1, (fn(parenthesized_item) do parenthesized_item + 1 end))
fn raising_source() -> { done: true, .. } | { done: false, value: integer, .. } raises "source" do
    raise "source"
end
let effect_iterator = iter.map(raising_source, fn(effect_item) do
    if effect_item > 0 then raise "callback" else effect_item end
end)
let while_result = iter.each_while(list.iter([1, 2]), fn(while_item) do
    if while_item == 2 then iter.break(while_item) else iter.continue(nil) end
end)
let folded_while = iter.fold_while(list.iter([1, 2]), 0, fn(while_state, fold_while_item) do
    if fold_while_item == 2 then iter.break("done")
    else iter.continue(while_state + fold_while_item)
    end
end)
let producer_flag: boolean = true
let repeated = iter.repeat_with(fn() do
    if producer_flag then raise "producer" else 1 end
end)
"#;
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    for name in ["folded", "generic_result", "parenthesized"] {
        assert_eq!(type_of(&inference, &resolution, name).display(), "integer");
    }
    assert_eq!(type_of(&inference, &resolution, "mixed").display(), "float");
    for name in ["piped", "trailing", "mapped", "filtered"] {
        assert_eq!(
            type_of(&inference, &resolution, name).display(),
            "[..integer]"
        );
    }
    assert_eq!(
        type_of(&inference, &resolution, "found").display(),
        "integer | nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "nil_items").display(),
        "[..(integer | nil)]"
    );
    assert_eq!(
        type_of(&inference, &resolution, "keys").display(),
        "[..(boolean | integer | float | string)]"
    );
    assert_eq!(
        type_of(&inference, &resolution, "exhausted_entry"),
        Type::Any
    );
    assert_eq!(
        type_of(&inference, &resolution, "live_entry").display(),
        "{ key: boolean | integer | float | string, value: any, .. }"
    );
    for name in [
        "acc",
        "fold_item",
        "pipeline_item",
        "trailing_item",
        "map_item",
        "filter_item",
        "find_item",
        "generic_item",
        "parenthesized_item",
        "effect_item",
    ] {
        assert_eq!(type_of(&inference, &resolution, name).display(), "integer");
    }
    assert_eq!(
        type_of(&inference, &resolution, "nil_item").display(),
        "integer | nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "effect_iterator").display(),
        "() -> { done: true, .. } | { done: false, value: integer, .. } raises \"source\" | \"callback\""
    );
    assert_eq!(
        type_of(&inference, &resolution, "while_result").display(),
        "2 | nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "folded_while").display(),
        "integer | \"done\""
    );
    assert_eq!(
        type_of(&inference, &resolution, "repeated").display(),
        "() -> { done: true, .. } | { done: false, value: integer, .. } raises \"producer\""
    );
    for name in ["while_item", "while_state", "fold_while_item"] {
        assert_eq!(type_of(&inference, &resolution, name).display(), "integer");
    }
}

#[test]
fn fold_accumulator_infers_nested_empty_lists_from_reducer_updates() {
    let db = AnalysisDatabase::default();
    let modules = [
        ("std/list", include_str!("../../../stdlib/list.simi")),
        ("std/iter", include_str!("../../../stdlib/iter.simi")),
    ]
    .into_iter()
    .map(|(name, source)| {
        let file = db.add_file(source);
        (name.to_owned(), simi_analysis::module_shape(&db, file))
    })
    .collect::<HashMap<_, _>>();
    let source = r#"let iter = require("std/iter")
alias number = integer | float

fn partition(ns: [..number], pivot: number)
    ns
    |> list.iter()
    |> iter.fold({lower=[], higher=[]}) <| fn(acc, n) do
        case acc of
            {lower, higher} when n < pivot =>
                {lower=lower |> tap list.append(n), higher}
            {lower, higher} =>
                {lower, higher=higher |> tap list.append(n)}
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
        type_of(&inference, &resolution, "partition").display(),
        "(ns: [..(integer | float)], pivot: integer | float) -> { lower: [..(integer | float)], higher: [..(integer | float)] }"
    );
    assert_eq!(
        type_of(&inference, &resolution, "acc").display(),
        "{ lower: [..(integer | float)], higher: [..(integer | float)] }"
    );
    for name in ["lower", "higher"] {
        assert_eq!(
            type_of(&inference, &resolution, name).display(),
            "[..(integer | float)]"
        );
    }
}

#[test]
fn generic_callback_without_element_evidence_preserves_exact_empty_list() {
    let source = r#"fn bridge<'state>(initial: 'state, callback: 'state -> 'state) -> 'state do
    callback(initial)
end
let inferred = bridge([], fn(xs) do xs end)
let unchanged: [] = bridge([], fn(other) do other end)
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    for name in ["inferred", "unchanged", "xs", "other"] {
        assert_eq!(type_of(&inference, &resolution, name).display(), "[]");
    }
}

#[test]
fn contextual_fold_accumulators_preserve_annotated_and_exact_list_failures() {
    let db = AnalysisDatabase::default();
    let modules = [
        ("std/list", include_str!("../../../stdlib/list.simi")),
        ("std/iter", include_str!("../../../stdlib/iter.simi")),
    ]
    .into_iter()
    .map(|(name, source)| {
        let file = db.add_file(source);
        (name.to_owned(), simi_analysis::module_shape(&db, file))
    })
    .collect::<HashMap<_, _>>();
    let source = r#"let iter = require("std/iter")
let sealed: {lower: [], higher: []} = {lower=[], higher=[]}
let sealed_result = iter.fold(list.iter([1]), sealed, fn(acc, n) do
    {lower=acc.lower |> tap list.append(n), higher=acc.higher}
end)
let partial = iter.fold(list.iter([1, 2.0]), {lower=[0], higher=[]}, fn(acc, n) do
    {lower=acc.lower |> tap list.append(n), higher=acc.higher}
end)
"#;
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &modules);
    assert_eq!(
        type_of(&inference, &resolution, "sealed_result").display(),
        "{ lower: [], higher: [] }"
    );
    assert_eq!(
        type_of(&inference, &resolution, "partial").display(),
        "{ lower: [integer], higher: [] }"
    );
    assert_eq!(
        inference
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                diagnostic.title.as_str(),
                diagnostic.detail.as_str(),
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                AnalysisDiagnosticCode::TypeMismatch,
                "Type mismatch",
                "Expected `[]`, but found `[integer]`.",
            ),
            (
                AnalysisDiagnosticCode::TypeMismatch,
                "Type mismatch",
                "Expected `{ lower: [], higher: [] }`, but found `{ lower: [integer], higher: [] }`.",
            ),
            (
                AnalysisDiagnosticCode::TypeMismatch,
                "Type mismatch",
                "Expected `[integer]`, but found `[integer, integer | float]`.",
            ),
            (
                AnalysisDiagnosticCode::TypeMismatch,
                "Type mismatch",
                "Expected `{ lower: [integer], higher: [] }`, but found `{ lower: [integer, integer | float], higher: [] }`.",
            ),
        ]
    );
}

#[test]
fn contextual_callbacks_still_check_explicit_annotations() {
    let db = AnalysisDatabase::default();
    let modules = [
        ("std/list", include_str!("../../../stdlib/list.simi")),
        ("std/iter", include_str!("../../../stdlib/iter.simi")),
    ]
    .into_iter()
    .map(|(name, source)| {
        let file = db.add_file(source);
        (name.to_owned(), simi_analysis::module_shape(&db, file))
    })
    .collect::<HashMap<_, _>>();
    let source = r#"
let iter = require("std/iter")
let compatible = iter.to_list(iter.map(list.iter([1, 2]), fn(item: integer) -> integer noraise do
    item + 1
end))
iter.fold(list.iter([1, 2]), 0, fn(acc: string, item: integer) do acc end)
iter.map(list.iter([1, 2]), fn(item: integer) -> string do item + 1 end)
iter.map(list.iter([1, 2]), fn(item: integer) -> any noraise do raise "nope" end)
"#;
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &modules);
    assert_eq!(
        type_of(&inference, &resolution, "compatible").display(),
        "[..integer]"
    );
    assert!(
        inference
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == AnalysisDiagnosticCode::TypeMismatch)
            .count()
            == 4,
        "{:?}",
        inference.diagnostics
    );
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
    case result of
    { kind = "ok", value = payload } =>
        payload
    { kind = "error", error = message } =>
        message
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
    let selected_case = case 1 of 1 => do value? 1 end end
    let protected = do value? 1 catch of _ => 2 end
    let caught = do raise "failure" catch of _ => do value? 1 end end
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
        "(value: integer | nil) -> integer | nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "anonymous").display(),
        "(value: integer | nil) -> integer | nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "boundaries").display(),
        "(value: integer | nil) -> \"continued\""
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
        "(value: string | nil) -> string | nil"
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
    let source = r#"
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
fn analyzed_calls_preserve_arguments_while_unknown_calls_widen() {
    let db = AnalysisDatabase::default();
    let module_file = db.add_file(include_str!("../../../stdlib/list.simi"));
    let modules = HashMap::from([(
        "std/list".to_owned(),
        simi_analysis::module_shape(&db, module_file),
    )]);
    let source = r#"

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
        "[integer, integer]"
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
let values: [..(integer | string)] = [1, "two"]
let tail = case values of
[1, ..rest] =>
    rest
_ =>
    []
end
let closed = {present = 1}
let result = case closed of
{missing = missing} =>
    missing
_ =>
    "fallback"
end
let extra = {x = 1, y = 2}
let closed_result = case extra of
{x = 1} =>
    "wrong"
_ =>
    "closed"
end
let open_result = case extra of
{x = value, ..} =>
    "open"
_ =>
    "wrong"
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
        "[..(integer | string)]"
    );
    assert_eq!(
        type_of(&inference, &resolution, "tail").display(),
        "[..(integer | string)]"
    );
    assert_eq!(
        type_of(&inference, &resolution, "result").display(),
        "\"fallback\""
    );
    assert_eq!(
        type_of(&inference, &resolution, "closed_result").display(),
        "\"closed\""
    );
    assert_eq!(
        type_of(&inference, &resolution, "open_result").display(),
        "\"open\""
    );
}

#[test]
fn structural_map_pattern_shorthand_requires_presence_and_binds_present_fields() {
    let source = r#"
let case_absent = case {} of
{case_missing} =>
    "wrong"
_ =>
    0
end
let case_present = case {case_value = 1} of
{case_value} =>
    case_value
_ =>
    "wrong"
end
let catch_absent = do
    raise {}
catch of
    {catch_missing} =>
        "wrong"
    _ =>
        0
end
let catch_present = do
    raise {catch_value = 2}
catch of
    {catch_value} =>
        catch_value
end
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    for name in [
        "case_absent",
        "case_present",
        "case_value",
        "catch_absent",
        "catch_present",
        "catch_value",
    ] {
        assert_eq!(
            type_of(&inference, &resolution, name).display(),
            "integer",
            "{name}"
        );
    }
}

#[test]
fn map_patterns_respect_optional_presence_and_all_required_fields() {
    let source = r#"
let absent = {missing = nil}
let absent_binding = case absent of
{missing = value} =>
    "present"
_ =>
    "absent"
end
let absent_nil = case absent of
{missing = nil} =>
    "nil"
_ =>
    "other"
end
fn maybe(value: string | nil) do
    let record = {maybe = value}
    case record of
    {maybe = present} =>
        "present"
    _ =>
        "absent"
    end
end
fn indexed(record: {[string]: integer}) do
    case record of
    {missing = value} =>
        "present"
    _ =>
        "absent"
    end
end
fn opened(record: {..}) do
    case record of
    {missing = value} =>
        "present"
    _ =>
        "absent"
    end
end
fn multiple(record: {first: "yes", second: "ok" | "no"}) do
    case record of
    {first = "yes", second = "ok"} =>
        "matched"
    _ =>
        "fallback"
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
                "maybe" => "(value: string | nil) -> \"present\" | \"absent\"",
                "indexed" => "(record: { [string]: integer }) -> \"present\" | \"absent\"",
                _ => "(record: { .. }) -> \"present\" | \"absent\"",
            }
        );
    }
    assert_eq!(
        type_of(&inference, &resolution, "multiple").display(),
        "(record: { first: \"yes\", second: \"ok\" | \"no\" }) -> \"matched\" | \"fallback\""
    );
}

#[test]
fn unannotated_case_patterns_seed_body_stable_list_and_map_domains() {
    let source = r#"
fn first_or_nil(values) do
    case values of
    [value, ..rest] =>
        value
    [] =>
        nil
    end
end
fn read_value(record) do
    case record of
    {value} =>
        value
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
        "(values: [..'a]) -> 'a | nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "read_value").display(),
        "(record: { value: 'a, .. }) -> 'a"
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
        "(flag: boolean) -> integer"
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
fn bounded_callable_generics_validate_calls_and_support_bounded_operators() {
    let source = r#"
fn negate<'a: integer | float>(value: 'a) -> 'a noraise do
    -value
end
let integer_result = negate(1)
let float_result = negate(1.5)
let invalid_result = negate("wrong")
"#;
    let (inference, resolution) = inferred(source);
    assert_eq!(
        type_of(&inference, &resolution, "integer_result"),
        Type::Int
    );
    assert_eq!(
        type_of(&inference, &resolution, "float_result"),
        Type::Float
    );
    assert_eq!(
        type_of(&inference, &resolution, "invalid_result"),
        Type::LiteralString("wrong".to_owned())
    );
    assert_eq!(
        inference.diagnostics.len(),
        1,
        "{:?}",
        inference.diagnostics
    );
    assert!(inference.diagnostics[0].detail.contains("integer | float"));
    assert_eq!(
        type_of(&inference, &resolution, "negate").display(),
        "<'a: integer | float> (value: 'a) -> 'a noraise"
    );
}

#[test]
fn nested_callable_generic_headers_shadow_outer_binders_and_preserve_unbounded_entries() {
    let source = r#"
fn use<'a: any>(
    value: 'a,
    callback: <'a: integer> 'a -> 'a noraise,
) -> 'a noraise do
    callback(1)
    value
end
fn marker<'a>() -> integer noraise do 1 end
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "use").display(),
        "<'a: any> (value: 'a, callback: <'b: integer> 'b -> 'b noraise) -> 'a noraise"
    );
    assert_eq!(
        type_of(&inference, &resolution, "marker").display(),
        "<'a> () -> integer noraise"
    );

    let invalid = r#"
fn invalid(callback: <'a: integer> 'a -> 'a noraise) -> nil noraise do
    callback("wrong")
    nil
end
"#;
    let (invalid_inference, _) = inferred(invalid);
    assert_eq!(invalid_inference.diagnostics.len(), 1);
    assert!(invalid_inference.diagnostics[0].detail.contains("integer"));
}

#[test]
fn aliases_with_nested_callable_headers_do_not_capture_outer_generics() {
    let source = r#"
alias handler<'value> = <'item> ('value, 'item) -> 'item noraise
fn hold<'a, 'b>(callback: handler<'a>, other: 'b) -> 'b noraise do other end
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "hold").display(),
        "<'a, 'b> (callback: <'c> ('a, 'c) -> 'c noraise, other: 'b) -> 'b noraise"
    );
}

#[test]
fn callable_labels_are_metadata_and_calls_remain_positional() {
    let source = r#"
fn add(left: integer, right: integer) -> integer noraise do
    left + right
end
let result = add(1, 2)
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(type_of(&inference, &resolution, "result"), Type::Int);
    assert_eq!(
        type_of(&inference, &resolution, "add").display(),
        "(left: integer, right: integer) -> integer noraise"
    );
}

#[test]
fn require_and_raised_callbacks_propagate_effects_with_raised_path_mutation() {
    let source = r#"
fn load(name: string) do require(name) end
let values = {}
let callback = fn() do
    values.item = 1
    raise "bad"
end
let observed = do
    callback()
catch of
    "bad" =>
        values
end
"#;
    let (inference, resolution) = inferred(source);
    assert_eq!(
        type_of(&inference, &resolution, "load").display(),
        "(name: string) -> any raises any"
    );
    assert_eq!(
        type_of(&inference, &resolution, "observed").display(),
        "{ .. }"
    );
}

#[test]
fn raised_effects_infer_propagate_and_are_removed_by_definite_catches() {
    let source = r#"
fn fail(value: 'e) do
    raise value
end
fn choose(flag: boolean) do
    if flag then raise "bad" else 1 end
end
fn invoke(callback: () -> integer raises 'e) do
    callback()
end
fn recovered() do
    do
        fail("bad")
    catch of
        "bad" =>
            1
    end
end
fn pure() -> integer noraise do
    1
end
fn invalid() -> integer noraise do
    raise "forbidden"
end
"#;
    let (inference, resolution) = inferred(source);
    assert_eq!(
        type_of(&inference, &resolution, "fail").display(),
        "(value: 'a) -> never raises 'a"
    );
    assert_eq!(
        type_of(&inference, &resolution, "choose").display(),
        "(flag: boolean) -> integer raises \"bad\""
    );
    assert_eq!(
        type_of(&inference, &resolution, "invoke").display(),
        "(callback: () -> integer raises 'a) -> integer raises 'a"
    );
    assert_eq!(
        type_of(&inference, &resolution, "recovered").display(),
        "() -> integer"
    );
    assert_eq!(
        type_of(&inference, &resolution, "pure").display(),
        "() -> integer noraise"
    );
    assert_eq!(
        inference.diagnostics.len(),
        1,
        "{:?}",
        inference.diagnostics
    );
    assert!(inference.diagnostics[0].detail.contains("never"));
}

#[test]
fn varied_direct_bodies_honor_noraise_without_suppressing_raises() {
    let source = r#"
fn identity(value: integer) -> integer noraise value
fn text() -> string noraise "ok"
fn values() -> [..integer] noraise [1, 2]
fn nothing() -> nil noraise nil
fn grouped() -> integer noraise (1 + 2)
fn direct(xs: [..integer]) -> nil noraise host.append(xs)
fn explicit(xs: [..integer]) -> nil noraise do
    host.append(xs)
end
fn unrelated() -> nil noraise raise "boom"
"#;
    let (inference, resolution) = inferred(source);
    for (name, expected) in [
        ("identity", "(value: integer) -> integer noraise"),
        ("text", "() -> string noraise"),
        ("values", "() -> [..integer] noraise"),
        ("nothing", "() -> nil noraise"),
        ("grouped", "() -> integer noraise"),
        ("direct", "(xs: [..integer]) -> nil noraise"),
        ("explicit", "(xs: [..integer]) -> nil noraise"),
    ] {
        assert_eq!(type_of(&inference, &resolution, name).display(), expected);
    }
    assert_eq!(
        inference.diagnostics.len(),
        1,
        "{:?}",
        inference.diagnostics
    );
    assert!(inference.diagnostics[0].detail.contains("never"));
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

#[test]
fn panic_and_todo_are_never_and_todo_warns_without_a_raised_effect() {
    let source = r#"
fn panicked() -> never do panic end
fn unfinished() -> never do todo "finish the decoder" end
"#;
    let (inference, resolution) = inferred(source);
    assert_eq!(
        type_of(&inference, &resolution, "panicked").display(),
        "() -> never"
    );
    assert_eq!(
        type_of(&inference, &resolution, "unfinished").display(),
        "() -> never"
    );
    assert_eq!(
        inference.diagnostics.len(),
        1,
        "{:?}",
        inference.diagnostics
    );
    let diagnostic = &inference.diagnostics[0];
    assert_eq!(diagnostic.code.as_str(), "todo");
    assert_eq!(
        diagnostic.severity,
        simi_analysis::AnalysisDiagnosticSeverity::Warning
    );
}

#[test]
fn map_local_binding_shorthand_infers_the_referenced_value_type() {
    let source = "let first = 1 let second = \"two\" let map = {first, second}";
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "map").display(),
        "{ first: integer, second: \"two\" }"
    );
}

#[test]
fn destructuring_let_patterns_report_match_certainty_without_changing_bindings() {
    let source = r#"
alias maybe_values = [..integer] | nil
let [first, second] = [1, 2]
let [head, ..tail] = [1, 2]
let {name = "Ada"} = {name = "Ada"}
let {missing = nil} = {}
let [1, {kind = "ok"}] = [1, {kind = "ok"}]
let [impossible, ..rest] = 42
let {missing = value} = {}
let ["two"] = ["one"]
fn unknown(values: any) do
    let [first, ..rest] = values
    first
end
fn unioned(values: maybe_values) do
    let [first, ..rest] = values
    first
end
fn open_map(values: {..}) do
    let {name = name} = values
    name
end
"#;
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &HashMap::new());
    let reported = diagnostics(&db, file);
    let codes = reported
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code,
                diagnostic.severity,
                diagnostic.detail.as_str(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        codes
            .iter()
            .filter(|(code, _, _)| *code == AnalysisDiagnosticCode::DestructuringLetNeverMatches)
            .count(),
        2,
        "{reported:?}"
    );
    assert_eq!(
        codes
            .iter()
            .filter(|(code, _, _)| *code == AnalysisDiagnosticCode::DestructuringLetMayFail)
            .count(),
        4,
        "{reported:?}"
    );
    assert!(
        codes.iter().all(|(code, severity, detail)| match code {
            AnalysisDiagnosticCode::DestructuringLetNeverMatches => {
                *severity == AnalysisDiagnosticSeverity::Error && detail.contains("incompatible")
            }
            AnalysisDiagnosticCode::DestructuringLetMayFail => {
                *severity == AnalysisDiagnosticSeverity::Warning && detail.contains("Use `case`")
            }
            _ => true,
        }),
        "{reported:?}"
    );
    assert_eq!(
        type_of(&inference, &resolution, "first").display(),
        "integer"
    );
    assert_eq!(type_of(&inference, &resolution, "value").display(), "nil");
}

#[test]
fn map_destructuring_let_binding_fields_infer_absence_as_nil() {
    let source = r#"
let {present, missing, source = renamed} = {present = 1}
let {nested = {nested_missing}} = {nested = {}}
let [{list_missing}] = [{}]
fn indexed(values: {[string]: integer}) do
    let {value, ..} = values
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
        type_of(&inference, &resolution, "present").display(),
        "integer"
    );
    assert_eq!(type_of(&inference, &resolution, "missing").display(), "nil");
    assert_eq!(type_of(&inference, &resolution, "renamed").display(), "nil");
    assert_eq!(
        type_of(&inference, &resolution, "nested_missing").display(),
        "nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "list_missing").display(),
        "nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "indexed").display(),
        "(values: { [string]: integer }) -> integer | nil"
    );
}

#[test]
fn closed_map_destructuring_over_unknown_keys_retains_extra_key_failure() {
    let closed_source = r#"
fn indexed(values: {[string]: integer}) do
    let {value} = values
    value
end
fn open(values: {..}) do
    let {value} = values
    value
end
"#;
    let db = AnalysisDatabase::default();
    let file = db.add_file(closed_source);
    let reported = diagnostics(&db, file);
    assert_eq!(reported.len(), 2, "{reported:?}");
    assert!(reported.iter().all(|diagnostic| {
        diagnostic.code == AnalysisDiagnosticCode::DestructuringLetMayFail
            && diagnostic.severity == AnalysisDiagnosticSeverity::Warning
    }));

    let rest_source = r#"
fn indexed(values: {[string]: integer}) do
    let {value, ..} = values
    value
end
fn open(values: {..}) do
    let {value, ..rest} = values
    [value, rest]
end
"#;
    let (inference, _) = inferred(rest_source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
}

#[test]
fn nil_union_map_field_allows_absent_annotated_presence() {
    let source = r#"
let state: {count: integer | nil} = {}
state.count = 1
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "state").display(),
        "{ count: integer }"
    );
}

#[test]
fn closure_capture_requires_a_stable_structural_contract() {
    let unannotated = r#"
let state = {}
let initialize = fn() do state.count = 1 end
"#;
    let (inference, _) = inferred(unannotated);
    assert!(
        inference
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.title == "Captured mutation exceeds declared type")
    );

    let annotated = r#"
let state: {count: integer | nil} = {}
let initialize = fn() do state.count = 1 end
"#;
    let (inference, _) = inferred(annotated);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
}

#[test]
fn captured_map_index_mutations_require_a_stable_structural_contract() {
    let source = r#"
let literal_state = {}
let set_literal = fn() do literal_state["count"] = 1 end
let dynamic_state = {}
let key = "count"
let set_dynamic = fn() do dynamic_state[key] = 1 end
"#;
    let (inference, _) = inferred(source);
    assert_eq!(
        inference
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.title == "Captured mutation exceeds declared type")
            .count(),
        2,
        "{:?}",
        inference.diagnostics
    );
}
