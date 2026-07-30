use std::collections::HashMap;

use simi_analysis::{
    AnalysisDatabase, AnalysisDiagnosticCode, AnalysisDiagnosticSeverity, ModuleShape, Type,
    diagnostics, expression_type_at, infer_types, module_shape, parse, resolve, symbol_type_at,
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

fn inferred_with_modules(
    source: &str,
    modules: &HashMap<String, ModuleShape>,
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
    (infer_types(&db, file, modules), resolution)
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
fn bytes_annotations_indexing_equality_and_narrowing_are_erased_static_facts() {
    let source = r#"
fn first(value: bytes)
    value[0]
fn equal(left: bytes, right: bytes)
    left == right
fn narrow(value: bytes | string)
    if type(value) == "bytes" then value[0] else value end
"#;
    let (inference, resolution) = inferred(source);

    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(Type::Bytes.display(), "bytes");
    assert_eq!(
        type_of(&inference, &resolution, "first").display(),
        "fn(value: bytes) -> integer | nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "equal").display(),
        "fn(left: bytes, right: bytes) -> boolean"
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "value", 4),
        "bytes"
    );
}

#[test]
fn bytes_literals_infer_bytes_and_reject_dynamic_text_segments() {
    let source = r#"
let data = #[0, "PNG", 255]
fn append(prefix: bytes)
    #[0, "PNG", prefix]
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(type_of(&inference, &resolution, "data"), Type::Bytes);
    assert_eq!(
        type_of(&inference, &resolution, "append").display(),
        "fn(prefix: bytes) -> bytes"
    );

    let (inference, _) = inferred("let text = \"PNG\" let data = #[text]");
    assert!(
        inference
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == AnalysisDiagnosticCode::TypeMismatch),
        "{:?}",
        inference.diagnostics
    );
}

#[test]
fn bytes_patterns_constrain_scrutinees_and_infer_capture_bindings() {
    let source = r#"
let #[byte, fixed:2, ..remaining] = #[1, 2, 3, 4]
let selected = case #["PNG", 1, 2] of
    #["PNG", version, ..data] => [version, data]
end
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(type_of(&inference, &resolution, "byte"), Type::Int);
    assert_eq!(type_of(&inference, &resolution, "fixed"), Type::Bytes);
    assert_eq!(type_of(&inference, &resolution, "remaining"), Type::Bytes);
    assert_eq!(
        type_of(&inference, &resolution, "selected").display(),
        "[integer, bytes]"
    );
}

#[test]
fn impossible_bytes_let_patterns_remain_analysis_errors() {
    let (inference, _) = inferred("let #[byte] = \"not bytes\"");
    assert!(
        inference.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == AnalysisDiagnosticCode::DestructuringLetNeverMatches
        }),
        "{:?}",
        inference.diagnostics
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
fn next<'a>(item: 'a, stop: boolean) -> Step<'a>
    if stop then {done = true} else {done = false, value = item} end
fn read(step: Step<integer>) -> integer | nil
    if step.done then
        let exhausted_value = step.value
        exhausted_value
    else
        let payload = step.value
        payload
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
fn accept(value: 42) -> integer
    value
fn exact_result() -> 42
    42
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
let annotated: fn() -> true ! never = fn() true
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
        "fn() -> true"
    );
    assert_eq!(
        type_of(&inference, &resolution, "direct_int").display(),
        "fn() -> 42"
    );
    assert_eq!(
        type_of(&inference, &resolution, "anon").display(),
        "fn() -> false"
    );
    assert_eq!(
        type_of(&inference, &resolution, "annotated").display(),
        "fn() -> true ! never"
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
let inferred: fn(value: true) -> true ! never = fn(value) value
let raised: fn() -> never ! string = fn() raise "failure"
let explicit_result_mismatch: fn() -> true ! never = fn() -> false ! never false
let effect_mismatch: fn() -> true ! never = fn() raise "failure"
"#;
    let (inference, resolution) = inferred(source);
    assert_eq!(
        type_of(&inference, &resolution, "inferred").display(),
        "fn(value: true) -> true ! never"
    );
    assert_eq!(
        type_of(&inference, &resolution, "raised").display(),
        "fn() -> never ! string"
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

#[test]
fn lisp_example_is_a_fully_typed_recursive_interpreter() {
    let source = include_str!("../../../examples/lisp.simi");
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "type diagnostics: {:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "final_environment"),
        Type::Named("Environment".to_owned())
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

fn type_of_any(
    inference: &simi_analysis::TypeInference,
    resolution: &simi_analysis::Resolution,
    name: &str,
    occurrence: usize,
) -> Type {
    let symbol = resolution
        .hir
        .symbols
        .iter()
        .filter(|(_, symbol)| symbol.name == name)
        .nth(occurrence)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("missing symbol {name} occurrence {occurrence}"));
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
fn process(n)
    n + 1
fn increment(n: integer)
    n + 1
fn identity(value)
    value
fn mixed_generics(explicit: 'a, inferred)
    inferred
fn choose(flag, value)
    if flag then value else nil end
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
        "fn(n: integer | float) -> integer | float"
    );
    assert_eq!(
        type_of(&inference, &resolution, "increment").display(),
        "fn(n: integer) -> integer"
    );
    assert_eq!(
        type_of(&inference, &resolution, "identity").display(),
        "fn(value: 'a) -> 'a"
    );
    assert_eq!(
        type_of(&inference, &resolution, "mixed_generics").display(),
        "fn(explicit: 'a, inferred: 'b) -> 'b"
    );
    assert_eq!(
        type_of(&inference, &resolution, "choose").display(),
        "fn(flag: boolean, value: 'a) -> 'a | nil"
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
fn combine(value: integer, suffix: string) -> string
    suffix
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
let callback: fn(integer) -> string | nil = fn(value: integer) -> string | nil
    if value == 0 then nil else "value" end
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
        "fn(integer) -> string | nil"
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
fn closed_recursive_records_keep_declared_fields_out_of_index_signatures() {
    let source = r#"
type Environment = {
    values: {[string]: integer},
    parent: Environment | nil,
}

fn environment(parent: Environment | nil) -> Environment
    {values = {}, parent = parent}

let root: Environment = environment(nil)
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "root"),
        Type::Named("Environment".to_owned())
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
    let module_file = db.add_file(
        "fn append(xs: [..'a], x: 'a) -> nil
    nil { append = append }",
    );
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
let before = fn()
    value
let value = "new"
let after_value = fn()
    value"#;
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
        "fn() -> integer"
    );
    assert_eq!(
        type_of(&inference, &resolution, "after_value").display(),
        "fn() -> \"new\""
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
        "let mapped = iter.to_list(iter.map(list.iter([1, 2]), fn(value)
    value + 1))\n",
        "let found = iter.find(list.iter([1, 2]), fn(value)
    value > 1)\n",
        "let enumerated = iter.to_list(iter.enumerate(iter.range(0, 2)))\n",
        "let zipped = iter.to_list(iter.zip(iter.repeat(1, 2), iter.once(\"x\")))\n",
        "let longest = iter.to_list(iter.zip_longest(iter.once(1), iter.take(iter.once(\"x\"), 0), nil))\n",
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
    assert_eq!(
        type_of(&inference, &resolution, "enumerated").display(),
        "[..[integer, integer]]"
    );
    assert_eq!(
        type_of(&inference, &resolution, "zipped").display(),
        "[..[integer, \"x\"]]"
    );
    assert_eq!(
        type_of(&inference, &resolution, "longest").display(),
        "[..[integer | nil, \"x\" | nil]]"
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
let folded = iter.fold(list.iter([1, 2, 3]), 0, fn(acc, fold_item)
    acc + fold_item)
let piped =
    [1, 2, 3]
    |> list.iter()
    |> iter.map(fn(pipeline_item)
        pipeline_item + 1)
    |> iter.to_list()
let trailing_iterator =
    iter.map(list.iter([1, 2, 3])) <| fn(trailing_item)
        trailing_item + 1
let trailing = iter.to_list(trailing_iterator)
let mixed = iter.fold(list.iter([1, 2.0]), 0.0, fn(mixed_acc, mixed_item)
    mixed_acc + mixed_item)
let mapped = iter.to_list(iter.map(list.iter([1, 2]), fn(map_item)
    map_item + 1))
let filtered = iter.to_list(iter.filter(list.iter([1, 2]), fn(filter_item)
    filter_item > 1))
let found = iter.find(list.iter([1, 2]), fn(find_item)
    find_item > 1)
let nil_items = iter.to_list(iter.map(list.iter([1, nil, 3]), fn(nil_item)
    nil_item))
let keys =
    map.iter({first = 1})
    |> iter.map(fn(entry)
        entry.key)
    |> iter.to_list()
let map_step = iter.next(map.iter({}))
if map_step.done then
    let exhausted_entry = map_step.value
else
    let live_entry = map_step.value
end
fn transform<'a, 'b, 'e>(value: 'a, callback: fn('a) -> 'b ! 'e) -> 'b ! 'e
    callback(value)
let generic_result = transform(1, fn(generic_item)
    generic_item + 1)
let parenthesized = transform(1, (fn(parenthesized_item)
    parenthesized_item + 1))
fn raising_source() -> { done: true, .. } | { done: false, value: integer, .. } ! "source"
    raise "source"
let effect_iterator = iter.map(raising_source, fn(effect_item)
    if effect_item > 0 then raise "callback" else effect_item end)
let while_result = iter.each_while(list.iter([1, 2]), fn(while_item)
    if while_item == 2 then iter.break(while_item) else iter.continue(nil) end)
let folded_while = iter.fold_while(list.iter([1, 2]), 0, fn(while_state, fold_while_item)
    if fold_while_item == 2 then iter.break("done")
    else iter.continue(while_state + fold_while_item)
    end)
let producer_flag: boolean = true
let loop_result = iter.loop(fn()
    if producer_flag then iter.break("complete") else iter.continue(nil) end)
let repeated = iter.repeat_with(fn()
    if producer_flag then raise "producer" else 1 end)
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
        "fn() -> { done: true, .. } | { done: false, value: integer, .. } ! \"source\" | \"callback\""
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
        type_of(&inference, &resolution, "loop_result").display(),
        "\"complete\""
    );
    assert_eq!(
        type_of(&inference, &resolution, "repeated").display(),
        "fn() -> { done: true, .. } | { done: false, value: integer, .. } ! \"producer\""
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
    |> iter.fold({lower=[], higher=[]}) <| fn(acc, n)
        case acc of
            {lower, higher} when n < pivot =>
                {lower=lower |> tap list.append(n), higher}
            {lower, higher} =>
                {lower, higher=higher |> tap list.append(n)}
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
        "fn(ns: [..(integer | float)], pivot: integer | float) -> { lower: [..(integer | float)], higher: [..(integer | float)] }"
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
    let source = r#"fn bridge<'state>(initial: 'state, callback: fn('state) -> 'state) -> 'state
    callback(initial)
let inferred = bridge([], fn(xs)
    xs)
let unchanged: [] = bridge([], fn(other)
    other)
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
fn open_record_contexts_widen_unsealed_empty_map_fields() {
    let source = r#"
fn return_value() -> {value: {..}}
    {value = {}}

fn environment(parent: string) -> {values: {..}, parent: string}
    {values = {}, parent = parent}

fn accept(value: {value: {..}}) -> {value: {..}}
    value

let exact = {}
let assigned: {value: {..}} = {value = {}}
let argument = accept({value = {}})
let returned = return_value()
let environment_value = environment("parent")
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(type_of(&inference, &resolution, "exact").display(), "{}");
    assert_eq!(
        type_of(&inference, &resolution, "assigned").display(),
        "{ value: { .. } }"
    );
    assert_eq!(
        type_of(&inference, &resolution, "argument").display(),
        "{ value: { .. } }"
    );
    assert_eq!(
        type_of(&inference, &resolution, "returned").display(),
        "{ value: { .. } }"
    );
    assert_eq!(
        type_of(&inference, &resolution, "environment_value").display(),
        "{ values: { .. }, parent: string }"
    );
}

#[test]
fn maps_do_not_promise_required_any_fields_that_nil_deletion_can_remove() {
    let source = r#"
fn environment(parent: any) -> {values: {..}, parent: any}
    {values = {}, parent = parent}
"#;
    let (inference, _) = inferred(source);
    assert!(
        inference.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == AnalysisDiagnosticCode::TypeMismatch
                && diagnostic.detail.contains("{ values: {}, .. }")
        }),
        "{:?}",
        inference.diagnostics
    );
}

#[test]
fn open_records_preserve_required_fields_through_direct_mutation_and_recursion() {
    let source = r#"
fn advance(state: {index: integer, ..}) -> {index: integer, ..} do
    state.index = state.index + 1
    if state.index < 2 then advance(state) else state end
end

let cursor = advance({index = 0})
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "cursor").display(),
        "{ index: integer, .. }"
    );
}

#[test]
fn contextual_empty_maps_refine_generic_callback_state() {
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

fn two_sum(values: [..integer], target: integer)
    values
    |> list.iter()
    |> iter.enumerate()
    |> iter.fold_while({}) <| fn(seen, item) do
        let index = item[0]
        let value = item[1]
        let match_index = seen[target - value]
        if match_index == nil then
            seen[value] = index
            iter.continue(seen)
        else
            iter.break([match_index, index])
        end
    end

fn bridge<'state>(initial: 'state, callback: fn('state) -> 'state) -> 'state
    callback(initial)
let unchanged = bridge({}, fn(state)
    state)
let integer_key: integer = 1
let boolean_key: boolean = true
let integer_value: integer = 1
let string_value: string = "one"
let inspected = bridge({}, fn(state) do
    let missing = state[integer_key]
    state
end)
let named = bridge({seen = {}, total = 0}, fn(state) do
    let seen = state.seen
    seen[integer_key] = integer_value
    {seen = seen, total = state.total + 1}
end)
let multiple = bridge({}, fn(state) do
    state[integer_key] = integer_value
    state[boolean_key] = string_value
    state
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
    assert_eq!(
        type_of(&inference, &resolution, "two_sum").display(),
        "fn(values: [..integer], target: integer) -> [integer, integer] | { [integer]: integer }"
    );
    assert_eq!(
        type_of(&inference, &resolution, "seen").display(),
        "{ [integer]: integer }"
    );
    assert_eq!(
        type_of(&inference, &resolution, "unchanged").display(),
        "{}"
    );
    assert_eq!(
        type_of(&inference, &resolution, "inspected").display(),
        "{}"
    );
    assert_eq!(
        type_of(&inference, &resolution, "named").display(),
        "{ seen: { [integer]: integer }, total: integer }"
    );
    assert_eq!(
        type_of(&inference, &resolution, "multiple").display(),
        "{ [boolean | integer]: integer | string }"
    );
}

#[test]
fn contextual_empty_maps_reject_captured_and_explicitly_closed_writes() {
    let source = r#"fn bridge<'state>(initial: 'state, callback: fn('state) -> 'state) -> 'state
    callback(initial)
let captured = {}
let mutate_capture = fn(key)
    captured[key] = 1
let explicitly_closed: {} = bridge({}, fn(state) do
    state[1] = 2
    state
end)
"#;
    let (inference, _) = inferred(source);
    assert!(
        inference
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.title == "Captured mutation exceeds declared type"),
        "{:?}",
        inference.diagnostics
    );
    assert!(
        inference
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == AnalysisDiagnosticCode::TypeMismatch),
        "{:?}",
        inference.diagnostics
    );
}

#[test]
fn contextual_empty_maps_seal_captured_dynamic_writes_and_preserve_nil_deletes() {
    let source = r#"fn bridge<'state>(initial: 'state, callback: fn('state) -> 'state) -> 'state
    callback(initial)
let captured = bridge({}, fn(state) do
    let mutate = fn(key)
        state[key] = 1
    state
end)
let deleted = bridge({}, fn(state) do
    let key = "missing"
    state[key] = nil
    state
end)
let unchanged = bridge({}, fn(state)
    state)
"#;
    let (inference, resolution) = inferred(source);
    assert_eq!(
        inference
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.title == "Captured mutation exceeds declared type")
            .count(),
        1,
        "{:?}",
        inference.diagnostics
    );
    for name in ["captured", "deleted", "unchanged"] {
        assert_eq!(
            type_of(&inference, &resolution, name).display(),
            "{}",
            "{name}"
        );
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
let sealed_result = iter.fold(list.iter([1]), sealed, fn(acc, n)
    {lower=acc.lower |> tap list.append(n), higher=acc.higher})
let partial = iter.fold(list.iter([1, 2.0]), {lower=[0], higher=[]}, fn(acc, n)
    {lower=acc.lower |> tap list.append(n), higher=acc.higher})
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
let compatible = iter.to_list(iter.map(list.iter([1, 2]), fn(item: integer) -> integer ! never
    item + 1))
iter.fold(list.iter([1, 2]), 0, fn(acc: string, item: integer)
    acc)
iter.map(list.iter([1, 2]), fn(item: integer) -> string
    item + 1)
iter.map(list.iter([1, 2]), fn(item: integer) -> any ! never
    raise "nope")
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
fn one(value: integer) -> integer
    value
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
fn classify(value: integer | string | nil)
    if type(value) == "integer" then
        value
    elseif value == nil then
        "nil"
    else
        value
    end
fn read(item: result)
    if item.kind == "ok" then
        item.value
    else
        item.error
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
fn short_circuit_guards_narrow_rhs() {
    let source = r#"
fn choose(input: string | nil)
    if nil != input and (input == "x" or input == "y") then
        input
    else
        "other"
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
}

#[test]
fn shadowed_type_does_not_narrow_and_boolean_complements_are_local() {
    let source = r#"
fn type(ignored)
    "integer"
fn inspect(subject: integer | string)
    if not (type(subject) != "integer") then subject else subject end
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
fn mutate(item: outcome)
    if item.kind == "ok" then
        item.kind = "ok"
        item
    else
        item
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
fn unwrap(result: result)
    case result of
    { kind = "ok", value = payload } =>
        payload
    { kind = "error", error = message } =>
        message
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
    let selected_case = case 1 of 1 => do
        value?
        1
    end end
    let protected = do value? 1 catch _ => 2 end
    let caught = do raise "failure" catch _ => do
        value?
        1
    end end
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
        "fn(value: integer | nil) -> integer | nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "anonymous").display(),
        "fn(value: integer | nil) -> integer | nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "boundaries").display(),
        "fn(value: integer | nil) -> \"continued\""
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
        "fn(value: string | nil) -> string | nil"
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

fn opaque(value)
    value
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
fn forward_source_calls_preserve_annotated_mutable_arguments() {
    let db = AnalysisDatabase::default();
    let source = r#"
type Expr =
    | {kind: "integer", value: integer}
    | {kind: "list", items: [..Expr]}
fn caller(items: [..Expr]) -> [..Expr] do
    helper(items)
    items
end
fn helper(items: [..Expr]) -> [..Expr]
    items
"#;
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &HashMap::new());
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "items", 3),
        "[..Expr]"
    );
}

#[test]
fn forward_source_calls_widen_unconstrained_mutable_arguments() {
    let db = AnalysisDatabase::default();
    let source = r#"
fn caller(items: [..integer]) do
    helper(items)
    items
end
fn helper(items: any)
    items[0] = "wrong"
"#;
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let inference = infer_types(&db, file, &HashMap::new());
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_at(source, &inference, &resolution, "items", 2),
        "[..any]"
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
fn mutate(value: any)
    value
let values = [1, 2]
let hidden: any = values
mutate(hidden)
values
fn visit(value: integer) -> any
    value
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
catch
    {catch_missing} =>
        "wrong"
    _ =>
        0
end
let catch_present = do
    raise {catch_value = 2}
catch
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
fn indexed(record: {[string]: integer})
    case record of
    {missing = value} =>
        "present"
    _ =>
        "absent"
    end
fn opened(record: {..})
    case record of
    {missing = value} =>
        "present"
    _ =>
        "absent"
    end
fn multiple(record: {first: "yes", second: "ok" | "no"})
    case record of
    {first = "yes", second = "ok"} =>
        "matched"
    _ =>
        "fallback"
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
                "maybe" => "fn(value: string | nil) -> \"present\" | \"absent\"",
                "indexed" => "fn(record: { [string]: integer }) -> \"present\" | \"absent\"",
                _ => "fn(record: { .. }) -> \"present\" | \"absent\"",
            }
        );
    }
    assert_eq!(
        type_of(&inference, &resolution, "multiple").display(),
        "fn(record: { first: \"yes\", second: \"ok\" | \"no\" }) -> \"matched\" | \"fallback\""
    );
}

#[test]
fn unannotated_case_patterns_seed_body_stable_list_and_map_domains() {
    let source = r#"
fn first_or_nil(values)
    case values of
    [value, ..rest] =>
        value
    [] =>
        nil
    end
fn read_value(record)
    case record of
    {value} =>
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
        type_of(&inference, &resolution, "first_or_nil").display(),
        "fn(values: [..'a]) -> 'a | nil"
    );
    assert_eq!(
        type_of(&inference, &resolution, "read_value").display(),
        "fn(record: { value: 'a, .. }) -> 'a"
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
        "fn() -> never"
    );
    assert_eq!(
        type_of(&inference, &resolution, "eventually").display(),
        "fn(flag: boolean) -> integer"
    );
    assert_eq!(
        type_of(&inference, &resolution, "left").display(),
        "fn() -> never"
    );
    assert_eq!(
        type_of(&inference, &resolution, "right").display(),
        "fn() -> never"
    );
    assert_eq!(
        type_of(&inference, &resolution, "nested").display(),
        "fn() -> never"
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
fn shadowed_any_and_constant_boolean_reachability_are_preserved() {
    let source = r#"
let value: any = 1
value
let value = "later"
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
    assert_eq!(
        type_at(source, &inference, &resolution, "value", 2),
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
fn negate<'a: integer | float>(value: 'a) -> 'a ! never
    -value
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
        "fn<'a: integer | float>(value: 'a) -> 'a ! never"
    );
}

#[test]
fn nested_callable_generic_headers_shadow_outer_binders_and_preserve_unbounded_entries() {
    let source = r#"
fn use<'a: any>(
    value: 'a,
    callback: fn<'a: integer>('a) -> 'a ! never,
) -> 'a ! never do
    callback(1)
    value
end
fn marker<'a>() -> integer ! never
    1
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "use").display(),
        "fn<'a: any>(value: 'a, callback: fn<'b: integer>('b) -> 'b ! never) -> 'a ! never"
    );
    assert_eq!(
        type_of(&inference, &resolution, "marker").display(),
        "fn<'a>() -> integer ! never"
    );

    let invalid = r#"
fn invalid(callback: fn<'a: integer>('a) -> 'a ! never) -> nil ! never do
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
alias handler<'value> = fn<'item>('value, 'item) -> 'item ! never
fn hold<'a, 'b>(callback: handler<'a>, other: 'b) -> 'b ! never
    other
"#;
    let (inference, resolution) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "hold").display(),
        "fn<'a, 'b>(callback: fn<'c>('a, 'c) -> 'c ! never, other: 'b) -> 'b ! never"
    );
}

#[test]
fn callable_labels_are_metadata_and_calls_remain_positional() {
    let source = r#"
fn add(left: integer, right: integer) -> integer ! never
    left + right
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
        "fn(left: integer, right: integer) -> integer ! never"
    );
}

#[test]
fn callable_or_nil_display_parenthesizes_function() {
    let source = r#"
fn nullable(flag: boolean)
    if flag then fn(value: integer)
        value end
"#;
    let (inference, resolution) = inferred(source);
    let ty = type_of(&inference, &resolution, "nullable");
    let compact = ty.display();
    assert_eq!(
        compact,
        "fn(flag: boolean) -> (fn(value: integer) -> integer) | nil"
    );
    let pretty = ty.pretty_display(80);
    assert!(
        pretty.contains("(fn(value: integer) -> integer)"),
        "wide pretty must include parenthesized callable, got {pretty:?}"
    );
    assert!(
        pretty.ends_with("| nil"),
        "wide pretty ends with union nil tail, got {pretty:?}"
    );
}

#[test]
fn callable_union_displays_roundtrippable_parenthesized_callable() {
    let source = r#"
fn choose(flag: boolean, callback: fn(integer) -> integer)
    if flag then callback else fn(value: integer)
        value end
"#;
    let (inference, resolution) = inferred(source);
    let ty = type_of(&inference, &resolution, "choose");
    let compact = ty.display();
    assert_eq!(
        compact,
        "fn(flag: boolean, callback: fn(integer) -> integer) -> fn(integer) -> integer"
    );
}

#[test]
fn require_and_raised_callbacks_propagate_effects_with_raised_path_mutation() {
    let source = r#"
fn load(name: string)
    require(name)
let values = {}
let callback = fn() do
    values.item = 1
    raise "bad"
end
let observed = do
    callback()
catch
    "bad" =>
        values
end
"#;
    let (inference, resolution) = inferred(source);
    assert_eq!(
        type_of(&inference, &resolution, "load").display(),
        "fn(name: string) -> any ! any"
    );
    assert_eq!(
        type_of(&inference, &resolution, "observed").display(),
        "{ .. }"
    );
}

#[test]
fn raised_effects_infer_propagate_and_are_removed_by_definite_catches() {
    let source = r#"
fn fail(value: 'e)
    raise value
fn choose(flag: boolean)
    if flag then raise "bad" else 1 end
fn invoke(callback: fn() -> integer ! 'e)
    callback()
fn recovered()
    do
        fail("bad")
    catch
        "bad" =>
            1
    end
fn pure() -> integer ! never
    1
fn invalid() -> integer ! never
    raise "forbidden"
"#;
    let (inference, resolution) = inferred(source);
    assert_eq!(
        type_of(&inference, &resolution, "fail").display(),
        "fn(value: 'a) -> never ! 'a"
    );
    assert_eq!(
        type_of(&inference, &resolution, "choose").display(),
        "fn(flag: boolean) -> integer ! \"bad\""
    );
    assert_eq!(
        type_of(&inference, &resolution, "invoke").display(),
        "fn(callback: fn() -> integer ! 'a) -> integer ! 'a"
    );
    assert_eq!(
        type_of(&inference, &resolution, "recovered").display(),
        "fn() -> integer"
    );
    assert_eq!(
        type_of(&inference, &resolution, "pure").display(),
        "fn() -> integer ! never"
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
fn varied_direct_bodies_honor_bang_never_without_suppressing_raises() {
    let source = r#"
fn identity(value: integer) -> integer ! never value
fn text() -> string ! never "ok"
fn values() -> [..integer] ! never [1, 2]
fn nothing() -> nil ! never nil
fn grouped() -> integer ! never (1 + 2)
fn direct(xs: [..integer]) -> nil ! never host.append(xs)
fn explicit(xs: [..integer]) -> nil ! never
    host.append(xs)
fn unrelated() -> nil ! never raise "boom"
"#;
    let (inference, resolution) = inferred(source);
    for (name, expected) in [
        ("identity", "fn(value: integer) -> integer ! never"),
        ("text", "fn() -> string ! never"),
        ("values", "fn() -> [..integer] ! never"),
        ("nothing", "fn() -> nil ! never"),
        ("grouped", "fn() -> integer ! never"),
        ("direct", "fn(xs: [..integer]) -> nil ! never"),
        ("explicit", "fn(xs: [..integer]) -> nil ! never"),
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
fn ignored(value: any, extra: any)
    value
fn kind(value: any) -> string
    type(value)
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
fn panicked() -> never
    panic
fn unfinished() -> never
    todo "finish the decoder"
"#;
    let (inference, resolution) = inferred(source);
    assert_eq!(
        type_of(&inference, &resolution, "panicked").display(),
        "fn() -> never"
    );
    assert_eq!(
        type_of(&inference, &resolution, "unfinished").display(),
        "fn() -> never"
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
fn destructuring_let_patterns_report_only_impossible_matches_without_changing_bindings() {
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
    assert!(
        codes.iter().all(|(code, severity, detail)| {
            *code == AnalysisDiagnosticCode::DestructuringLetNeverMatches
                && *severity == AnalysisDiagnosticSeverity::Error
                && detail.contains("incompatible")
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
        "fn(values: { [string]: integer }) -> integer | nil"
    );
}

#[test]
fn closed_map_destructuring_over_unknown_keys_remains_an_assertion() {
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
    assert!(reported.is_empty(), "{reported:?}");

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
let initialize = fn()
    state.count = 1
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
let initialize = fn()
    state.count = 1
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
let set_literal = fn()
    literal_state["count"] = 1
let dynamic_state = {}
let key = "count"
let set_dynamic = fn()
    dynamic_state[key] = 1
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

#[test]
fn portable_builtins_use_registered_module_shapes_for_global_type() {
    let db = AnalysisDatabase::default();
    let modules = [
        (
            "std/list",
            "fn append(xs, x)
    nil { append = append }",
        ),
        (
            "std/map",
            "fn clear(entries)
    nil { clear = clear }",
        ),
        (
            "std/number",
            "fn to_string(value)
    nil { to_string = to_string }",
        ),
        (
            "std/bytes",
            "fn length(data: bytes) -> integer
    0 { length = length }",
        ),
    ]
    .into_iter()
    .map(|(name, source)| {
        let file = db.add_file(source);
        (name.to_owned(), module_shape(&db, file))
    })
    .collect::<HashMap<_, _>>();

    let source = "list map number bytes";
    let (inference, resolution) = inferred_with_modules(source, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );

    for name in ["list", "map", "number", "bytes"] {
        let ty = type_of_any(&inference, &resolution, name, 0);
        assert!(
            !ty.display().contains("any"),
            "{name} should not be Any, got {ty:?}"
        );
    }
}

#[test]
fn portable_builtins_fallback_to_any_when_shapes_absent() {
    let source = "list map iter number string bytes";
    let (inference, resolution) = inferred(source);
    for name in ["list", "map", "iter", "number", "string", "bytes"] {
        assert_eq!(
            type_of_any(&inference, &resolution, name, 0).display(),
            "any",
            "{name} should be Any in bare context"
        );
    }
}

#[test]
fn literal_require_retains_precise_type_when_module_registered() {
    let db = AnalysisDatabase::default();
    let module_source = "let exports = { answer = 42, empty = {} } exports";
    let module_file = db.add_file(module_source);
    let modules = HashMap::from([("known".to_owned(), module_shape(&db, module_file))]);

    let source = "let data = require(\"known\")\ndata";
    let (inference, resolution) = inferred_with_modules(source, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "data").display(),
        "{ answer: integer, empty: {} }"
    );
}

#[test]
fn require_alias_retains_precise_type_when_module_registered() {
    let db = AnalysisDatabase::default();
    let module_source = "fn append(xs, x)
    nil { append = append }";
    let module_file = db.add_file(module_source);
    let modules = HashMap::from([("std/list".to_owned(), module_shape(&db, module_file))]);

    for source in [
        "let list = require(\"std/list\") list",
        "let list = require(\"std/list\") list.append",
    ] {
        let (inference, resolution) = inferred_with_modules(source, &modules);
        assert!(
            inference.diagnostics.is_empty(),
            "{:?}",
            inference.diagnostics
        );
        let ty = type_of(&inference, &resolution, "list");
        assert!(
            ty.display().contains("append"),
            "alias should carry module shape, got {ty:?}"
        );
    }
}

#[test]
fn shadowed_builtin_uses_user_binding_not_module_shape() {
    let db = AnalysisDatabase::default();
    let module_source = "fn append(xs, x)
    nil { append = append }";
    let module_file = db.add_file(module_source);
    let modules = HashMap::from([("std/list".to_owned(), module_shape(&db, module_file))]);

    let source = "let list = 42\nlist";
    let (inference, resolution) = inferred_with_modules(source, &modules);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
    assert_eq!(
        type_of(&inference, &resolution, "list").display(),
        "integer"
    );
}

#[test]
fn list_spreads_preserve_exact_and_rest_list_shapes() {
    let source = r#"
let exact = [1, ..[2, 3], "four"]
let tail: [..boolean] = []
let rest = [1, ..tail, "end"]
let invalid = [..1]
"#;
    let (inference, resolution) = inferred(source);
    assert_eq!(
        type_of(&inference, &resolution, "exact").display(),
        "[integer, integer, integer, \"four\"]"
    );
    assert_eq!(
        type_of(&inference, &resolution, "rest").display(),
        "[..(boolean | integer | \"end\")]"
    );
    assert!(
        inference
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == AnalysisDiagnosticCode::TypeMismatch),
        "{:?}",
        inference.diagnostics
    );
}

#[test]
fn named_recursive_types_keep_recursive_edges_collapsed_and_check_structure() {
    let source = r#"
type Expr =
    | {kind: "integer", value: integer}
    | {kind: "list", items: [..Expr]}
let leaf: Expr = {kind = "integer", value = 1}
let tree: Expr = {kind = "list", items = [leaf, {kind = "list", items = []}]}
let invalid: Expr = {kind = "unexpected", value = 1}
let invalid_nested: Expr = {kind = "list", items = ["wrong"]}
"#;
    let (inference, resolution) = inferred(source);
    assert_eq!(
        type_of(&inference, &resolution, "leaf"),
        Type::Named("Expr".to_owned())
    );
    assert_eq!(
        type_of(&inference, &resolution, "tree"),
        Type::Named("Expr".to_owned())
    );
    assert!(
        inference.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == AnalysisDiagnosticCode::TypeMismatch
                && diagnostic.detail.contains("Expr")
        }),
        "{:?}",
        inference.diagnostics
    );
    assert!(
        inference.diagnostics.len() < 5,
        "{:?}",
        inference.diagnostics
    );
}

#[test]
fn named_recursive_map_fields_narrow_nil_from_named_owners() {
    let source = r#"
type Environment = {parent: Environment | nil, ..}
fn parent_or_self(env: Environment) -> Environment
    if type(env.parent) == "nil" then env else env.parent end
"#;
    let (inference, _) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
}

#[test]
fn named_recursive_map_types_reject_deleting_required_fields() {
    let source = r#"
type Environment = {values: {..}, parent: Environment | nil, ..}
let env: Environment = {values = {}, parent = nil}
env.values = nil
let accepted: Environment = env
"#;
    let (inference, _) = inferred(source);
    assert!(
        inference.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == AnalysisDiagnosticCode::TypeMismatch
                && diagnostic.detail == "Expected `{ .. }`, but found `nil`."
        }),
        "{:?}",
        inference.diagnostics
    );
}

#[test]
fn named_recursive_map_types_allow_compatible_required_field_mutation() {
    let source = r#"
type Environment = {values: {..}, parent: Environment | nil, ..}
let env: Environment = {values = {}, parent = nil}
env.values = {count = 1}
"#;
    let (inference, _) = inferred(source);
    assert!(
        inference.diagnostics.is_empty(),
        "{:?}",
        inference.diagnostics
    );
}
