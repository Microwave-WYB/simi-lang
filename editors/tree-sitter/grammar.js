/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

const PREC = {
  ASSIGNMENT: 1,
  PIPELINE: 2,
  TRAILING_ARGUMENT: 3,
  OR: 4,
  AND: 5,
  EQUALITY: 6,
  COMPARISON: 7,
  CONCATENATION: 8,
  ADDITIVE: 9,
  MULTIPLICATIVE: 10,
  UNARY: 11,
  POSTFIX: 12,
};

module.exports = grammar({
  name: "simi",

  extras: ($) => [/[\s\uFEFF\u2060\u200B]/, $.comment],

  word: ($) => $.identifier,

  reserved: {
    global: (_) => [],
    expression: (_) => ["loop", "break", "continue"],
  },

  supertypes: ($) => [$._statement, $._expression, $._pattern],

  conflicts: ($) => [
    [$.assignment_target, $._primary_expression],
    [$.assignment_target, $._postfix_expression],
    [$.parenthesized_call, $._postfix_expression],
    [$._primary_expression, $.function_expression],
    [$._primary_expression, $.function_declaration],
    [$.parameters, $.callable_type_parameters],
    [$.declared_parameters, $.callable_type_parameters],
    [$.callable_type_parameter, $.type_annotation],
    [$.callable_type_parameter, $.declared_parameter],
    [$.callable_type_parameter, $.parameter],
  ],

  rules: {
    program: ($) => seq(
      optional($.requires_declaration),
      repeat($._statement),
    ),

    requires_declaration: ($) => seq(
      "requires",
      field("requirements", $.map),
    ),

    _statement: ($) => choice(
      $.function_declaration,
      $.alias_declaration,
      $.let_statement,
      $._expression,
    ),

    block: ($) => repeat1($._statement),

    function_declaration: ($) => seq(
      "fn",
      field("name", $.identifier),
      optional(field("type_parameters", $.callable_type_parameters)),
      field("parameters", $.declared_parameters),
      optional(seq(
        field("return_type", $.return_annotation),
        optional(field("effect", $.effect_annotation)),
      )),
      field("body", choice($.block_expression, $._expression)),
    ),

    alias_declaration: ($) => seq(
      "alias",
      field("name", $.identifier),
      optional(field("parameters", $.type_parameters)),
      "=",
      field("type", $._type),
    ),

    let_statement: ($) => seq(
      "let",
      field("pattern", $._let_pattern),
      optional(field("type", $.type_annotation)),
      "=",
      field("value", $._expression),
    ),

    _let_pattern: ($) => choice(
      $.identifier,
      $.integer,
      $.float,
      $.string,
      $.boolean,
      $.nil,
      $.list_pattern,
      $.bytes_pattern,
      $.map_pattern,
    ),

    _expression: ($) => choice(
      $.assignment_expression,
      $.pipeline_expression,
      $.trailing_argument_expression,
      $._logical_or_expression,
    ),

    assignment_expression: ($) => prec.right(PREC.ASSIGNMENT, seq(
      field("left", $.assignment_target),
      field("operator", "="),
      field("right", $._expression),
    )),

    assignment_target: ($) => choice(
      $.identifier,
      $.field_expression,
      $.index_expression,
      $.parenthesized_assignment_target,
    ),

    parenthesized_assignment_target: ($) => seq(
      "(",
      $.assignment_target,
      ")",
    ),

    pipeline_expression: ($) => prec.left(PREC.PIPELINE, seq(
      field("input", choice($.trailing_argument_expression, $._logical_or_expression)),
      repeat1($.pipeline_stage),
    )),

    pipeline_stage: ($) => seq(
      choice("|>", "?>"),
      optional("tap"),
      field("function", $.pipeline_callee),
      field("arguments", $.arguments),
      optional(seq(
        token(prec(1, "<|")),
        field("trailing_argument", choice(
          $.trailing_argument_expression,
          $._logical_or_expression,
        )),
      )),
    ),

    pipeline_callee: ($) => seq(
      $.identifier,
      repeat(seq(".", $._member_identifier)),
    ),

    trailing_argument_expression: ($) => prec.right(PREC.TRAILING_ARGUMENT, seq(
      field("call", choice($.call_expression, $.parenthesized_call)),
      token(prec(1, "<|")),
      field("argument", choice(
        $.trailing_argument_expression,
        $._logical_or_expression,
      )),
    )),

    parenthesized_call: ($) => seq(
      "(",
      choice($.call_expression, $.parenthesized_call),
      ")",
    ),

    _logical_or_expression: ($) => choice(
      $.binary_expression,
      $.unary_expression,
      $._postfix_expression,
    ),

    binary_expression: ($) => choice(
      ...[
        ["or", PREC.OR],
        ["and", PREC.AND],
        ["==", PREC.EQUALITY],
        ["!=", PREC.EQUALITY],
        ["<", PREC.COMPARISON],
        ["<=", PREC.COMPARISON],
        [">", PREC.COMPARISON],
        [">=", PREC.COMPARISON],
        ["+", PREC.ADDITIVE],
        ["-", PREC.ADDITIVE],
        ["*", PREC.MULTIPLICATIVE],
        ["/", PREC.MULTIPLICATIVE],
        ["//", PREC.MULTIPLICATIVE],
        ["%", PREC.MULTIPLICATIVE],
      ].map(([operator, precedence]) =>
        prec.left(precedence, seq(
          field("left", $._logical_or_expression),
          field("operator", operator),
          field("right", $._logical_or_expression),
        )),
      ).concat([
        prec.right(PREC.CONCATENATION, seq(
          field("left", $._logical_or_expression),
          field("operator", "<>"),
          field("right", $._logical_or_expression),
        )),
      ]),
    ),

    unary_expression: ($) => prec.right(PREC.UNARY, seq(
      field("operator", choice("-", "not")),
      field("operand", choice($.unary_expression, $._postfix_expression)),
    )),

    _postfix_expression: ($) => choice(
      $._primary_expression,
      $.call_expression,
      $.field_expression,
      $.index_expression,
      $.nil_propagation_expression,
    ),

    call_expression: ($) => prec.left(PREC.POSTFIX, seq(
      field("function", $._postfix_expression),
      field("arguments", $.arguments),
    )),

    field_expression: ($) => prec.left(PREC.POSTFIX, seq(
      field("object", $._postfix_expression),
      ".",
      field("name", $._member_identifier),
    )),

    _member_identifier: ($) => choice(
      $.identifier,
      alias("loop", $.identifier),
      alias("break", $.identifier),
      alias("continue", $.identifier),
    ),

    index_expression: ($) => prec.left(PREC.POSTFIX, seq(
      field("object", $._postfix_expression),
      token.immediate("["),
      field("index", $._expression),
      "]",
    )),

    nil_propagation_expression: ($) => prec.left(PREC.POSTFIX, seq(
      field("value", $._postfix_expression),
      "?",
    )),

    arguments: ($) => seq(
      "(",
      optional(commaSep1($._expression)),
      optional(","),
      ")",
    ),

    declared_parameters: ($) => seq(
      "(",
      optional(commaSep1($.declared_parameter)),
      optional(","),
      ")",
    ),

    declared_parameter: ($) => seq(
      field("name", $.identifier),
      optional(field("type", $.type_annotation)),
    ),

    parameters: ($) => seq(
      "(",
      optional(commaSep1($.parameter)),
      optional(","),
      ")",
    ),

    parameter: ($) => seq(
      field("name", $.identifier),
      optional(field("type", $.type_annotation)),
    ),

    type_annotation: ($) => seq(":", $._type),
    return_annotation: ($) => seq("->", $._type),

    type_parameters: ($) => seq(
      "<",
      optional(commaSep1($.type_variable)),
      optional(","),
      ">",
    ),

    callable_type_parameters: ($) => seq(
      "<",
      optional(commaSep1($.type_parameter)),
      optional(","),
      ">",
    ),

    type_parameter: ($) => seq(
      field("name", $.type_variable),
      optional(seq(":", field("constraint", $._type))),
    ),

    _type: ($) => $.callable_type,

    callable_type: ($) => prec.right(choice(
      seq(
        "fn",
        optional(field("type_parameters", $.callable_type_parameters)),
        field("parameters", $.callable_type_params),
        "->",
        field("result", $._type),
        optional(field("effect", $.effect_annotation)),
      ),
      $.union_type,
    )),

    callable_type_params: ($) => seq(
      "(",
      optional(commaSep1($.callable_type_parameter)),
      optional(","),
      ")",
    ),

    callable_type_parameter: ($) => seq(
      optional(seq(field("label", $.identifier), ":")),
      $._type,
    ),

    effect_annotation: ($) => seq(
      "!",
      field("type", $._type),
    ),

    union_type: ($) => seq(
      optional("|"),
      $._primary_type,
      repeat(seq("|", $._primary_type)),
    ),

    _primary_type: ($) => choice(
      $.named_type,
      $.type_variable,
      $.literal_type,
      $.parenthesized_type,
      $.list_type,
      $.map_type,
    ),

    named_type: ($) => prec.right(seq(
      field("name", $.identifier),
      optional(field("arguments", $.type_arguments)),
    )),

    type_arguments: ($) => seq(
      "<",
      optional(commaSep1($._type)),
      optional(","),
      ">",
    ),

    type_variable: ($) => token(seq("'", /[A-Za-z_][A-Za-z0-9_]*/)),

    literal_type: ($) => choice(
      $.string,
      $.nil,
      $.boolean,
      seq(optional("-"), choice($.integer, $.float)),
    ),

    parenthesized_type: ($) => seq(
      "(",
      $._type,
      ")",
    ),

    list_type: ($) => seq(
      "[",
      optional(choice(seq("..", $._type), commaSep1($._type))),
      optional(","),
      "]",
    ),

    map_type: ($) => seq(
      "{",
      optional(choice(
        seq(commaSep1($.map_type_entry), optional(seq(",", ".."))),
        "..",
      )),
      optional(","),
      "}",
    ),

    map_type_entry: ($) => choice(
      seq(field("name", $.identifier), ":", field("type", $._type)),
      seq("[", field("key", $._type), "]", ":", field("type", $._type)),
    ),

    _primary_expression: ($) => choice(
      reserved("expression", $.identifier),
      $.integer,
      $.float,
      $.string,
      $.boolean,
      $.nil,
      $.parenthesized_expression,
      $.list,
      $.bytes,
      $.map,
      $.function_expression,
      $.block_expression,
      $.if_expression,
      $.case_expression,
      $.protected_expression,
      $.raise_expression,
      $.panic_expression,
      $.todo_expression,
    ),

    parenthesized_expression: ($) => seq("(", $._expression, ")"),

    list: ($) => seq(
      "[",
      optional(commaSep1($._list_element)),
      optional(","),
      "]",
    ),

    _list_element: ($) => seq(
      optional(".."),
      $._expression,
    ),

    bytes: ($) => seq(
      "#",
      "[",
      optional(commaSep1($._expression)),
      optional(","),
      "]",
    ),

    map: ($) => seq(
      "{",
      optional(commaSep1($.map_entry)),
      optional(","),
      "}",
    ),

    map_entry: ($) => choice(
      $.map_field,
      $.map_shorthand,
      $.map_computed_entry,
    ),

    map_field: ($) => seq(
      field("name", $.identifier),
      "=",
      field("value", $._expression),
    ),

    map_shorthand: ($) => field("name", $.identifier),

    map_computed_entry: ($) => seq(
      "[",
      field("key", $._expression),
      "]",
      "=",
      field("value", $._expression),
    ),

    function_expression: ($) => seq(
      "fn",
      optional(field("type_parameters", $.callable_type_parameters)),
      field("parameters", $.parameters),
      optional(seq(
        field("return_type", $.return_annotation),
        optional(field("effect", $.effect_annotation)),
      )),
      field("body", choice($.block_expression, $._expression)),
    ),

    block_expression: ($) => seq(
      "do",
      optional(field("body", $.block)),
      "end",
    ),

    if_expression: ($) => seq(
      "if",
      field("condition", $._expression),
      "then",
      optional(field("consequence", $.block)),
      repeat($.elseif_clause),
      optional($.else_clause),
      "end",
    ),

    elseif_clause: ($) => seq(
      "elseif",
      field("condition", $._expression),
      "then",
      optional(field("consequence", $.block)),
    ),

    else_clause: ($) => seq(
      "else",
      optional(field("body", $.block)),
    ),

    case_expression: ($) => seq(
      "case",
      field("value", $._expression),
      "of",
      repeat1($.case_clause),
      "end",
    ),

    case_clause: ($) => seq(
      field("pattern", $._pattern),
      optional(seq("when", field("guard", $._expression))),
      "=>",
      field("body", $._expression),
    ),

    protected_expression: ($) => seq(
      "do",
      field("protected", $.block),
      "catch",
      "of",
      repeat1($.catch_arm),
      "end",
    ),

    catch_arm: ($) => seq(
      field("pattern", $._pattern),
      optional(seq("when", field("guard", $._expression))),
      "=>",
      field("body", $._expression),
    ),

    raise_expression: ($) => seq(
      "raise",
      field("value", $._expression),
    ),

    panic_expression: ($) => prec.right(seq("panic", optional(field("reason", $.string)))),

    todo_expression: ($) => prec.right(seq("todo", optional(field("note", $.string)))),

    _pattern: ($) => choice(
      $.wildcard_pattern,
      $.identifier,
      $.integer,
      $.float,
      $.string,
      $.boolean,
      $.nil,
      $.list_pattern,
      $.bytes_pattern,
      $.map_pattern,
    ),

    wildcard_pattern: (_) => token(prec(1, /_[A-Za-z0-9_]*/)),

    list_pattern: ($) => seq(
      "[",
      optional(choice(
        seq(
          commaSep1($._pattern),
          optional(seq(",", $.rest_pattern)),
          optional(","),
        ),
        seq($.rest_pattern, optional(",")),
      )),
      "]",
    ),

    bytes_pattern: ($) => seq(
      "#",
      "[",
      optional(choice(
        seq(
          commaSep1($._bytes_pattern_sized_segment),
          optional(seq(",", $.bytes_pattern_remainder)),
          optional(","),
        ),
        seq($.bytes_pattern_remainder, optional(",")),
      )),
      "]",
    ),

    _bytes_pattern_sized_segment: ($) => choice(
      $.string,
      field("name", choice($.wildcard_pattern, $.identifier)),
      $.bytes_pattern_fixed_capture,
    ),

    bytes_pattern_fixed_capture: ($) => seq(
      field("name", choice($.wildcard_pattern, $.identifier)),
      ":",
      "bytes",
      "(",
      field("length", $.integer),
      ")",
    ),

    bytes_pattern_remainder: ($) => seq(
      field("name", choice($.wildcard_pattern, $.identifier)),
      ":",
      "bytes",
    ),

    map_pattern: ($) => seq(
      "{",
      optional(choice(
        seq(
          commaSep1($.map_pattern_field),
          optional(seq(",", $.rest_pattern)),
          optional(","),
        ),
        seq($.rest_pattern, optional(",")),
      )),
      "}",
    ),

    map_pattern_field: ($) => seq(
      field("name", $.identifier),
      optional(seq(
        "=",
        field("pattern", $._pattern),
      )),
    ),

    rest_pattern: ($) => seq(
      "..",
      optional(field("name", choice($.wildcard_pattern, $.identifier))),
    ),

    boolean: (_) => choice("true", "false"),
    nil: (_) => "nil",

    float: (_) => token(choice(
      /[0-9](?:_?[0-9])*\.[0-9](?:_?[0-9])*(?:[eE][+-]?[0-9](?:_?[0-9])*)?/,
      /[0-9](?:_?[0-9])*[eE][+-]?[0-9](?:_?[0-9])*/,
    )),

    integer: (_) => token(choice(
      /0[bB]_?[01](?:_?[01])*/,
      /0[xX]_?[0-9a-fA-F](?:_?[0-9a-fA-F])*/,
      /[0-9](?:_?[0-9])*/,
    )),

    string: ($) => seq(
      '"',
      repeat(choice($.string_content, $.escape_sequence)),
      '"',
    ),

    string_content: (_) => token.immediate(/[^"\\]+/),
    escape_sequence: (_) => token.immediate(/\\["\\nrt]/),

    identifier: (_) => /[A-Za-z_][A-Za-z0-9_]*/,

    comment: (_) => token(seq("--", /[^\r\n]*/)),
  },
});

function commaSep1(rule) {
  return seq(rule, repeat(seq(",", rule)));
}
