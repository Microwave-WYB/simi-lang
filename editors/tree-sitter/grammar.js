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

  supertypes: ($) => [$._statement, $._expression, $._pattern],

  conflicts: ($) => [
    [$.assignment_target, $._primary_expression],
    [$.assignment_target, $._postfix_expression],
    [$.parenthesized_call, $._postfix_expression],
  ],

  rules: {
    program: ($) => repeat($._statement),

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
      field("parameters", $.declared_parameters),
      optional(field("return_type", $.return_annotation)),
      "do",
      optional(field("body", $.block)),
      "end",
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
        "<|",
        field("trailing_argument", choice(
          $.trailing_argument_expression,
          $._logical_or_expression,
        )),
      )),
    ),

    pipeline_callee: ($) => seq(
      $.identifier,
      repeat(seq(".", $.identifier)),
    ),

    trailing_argument_expression: ($) => prec.right(PREC.TRAILING_ARGUMENT, seq(
      field("call", choice($.call_expression, $.parenthesized_call)),
      "<|",
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
      field("name", $.identifier),
    )),

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
      optional(field("type", seq(":", $._type))),
    ),

    type_annotation: ($) => seq(
      ":",
      choice(
        seq(field("before", $.union_type), field("post", $.post_type)),
        $._type,
      ),
    ),
    return_annotation: ($) => seq("->", $._type),
    post_type: ($) => seq("=>", field("type", $.union_type)),

    type_parameters: ($) => seq(
      "<",
      optional(commaSep1($.type_variable)),
      optional(","),
      ">",
    ),

    _type: ($) => $.function_type,

    function_type: ($) => seq(
      $.union_type,
      optional(seq("->", $.function_type)),
    ),

    union_type: ($) => seq(
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

    literal_type: ($) => choice($.string, $.nil),

    parenthesized_type: ($) => seq(
      "(",
      optional(commaSep1($.function_type_parameter)),
      optional(","),
      ")",
    ),

    function_type_parameter: ($) => choice(
      seq(field("before", $.union_type), field("post", $.post_type)),
      $._type,
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
      $.identifier,
      $.integer,
      $.float,
      $.string,
      $.boolean,
      $.nil,
      $.parenthesized_expression,
      $.list,
      $.map,
      $.function_expression,
      $.block_expression,
      $.if_expression,
      $.loop_expression,
      $.case_expression,
      $.raise_expression,
      $.try_expression,
      $.continue_expression,
      $.break_expression,
    ),

    parenthesized_expression: ($) => seq("(", $._expression, ")"),

    list: ($) => seq(
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
      $.map_computed_entry,
    ),

    map_field: ($) => seq(
      field("name", $.identifier),
      "=",
      field("value", $._expression),
    ),

    map_computed_entry: ($) => seq(
      "[",
      field("key", $._expression),
      "]",
      "=",
      field("value", $._expression),
    ),

    function_expression: ($) => seq(
      "fn",
      field("parameters", $.parameters),
      optional(field("return_type", $.return_annotation)),
      "do",
      optional(field("body", $.block)),
      "end",
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

    loop_expression: ($) => seq(
      "loop",
      optional(seq(
        field("state", $.identifier),
        "=",
        field("initial", $._expression),
      )),
      "do",
      optional(field("body", $.block)),
      "end",
    ),

    continue_expression: ($) => prec.right(seq(
      "continue",
      optional(field("value", $._expression)),
    )),

    break_expression: ($) => seq(
      "break",
      field("value", $._expression),
    ),

    case_expression: ($) => seq(
      "case",
      field("value", $._expression),
      repeat1($.case_clause),
      "end",
    ),

    case_clause: ($) => seq(
      "of",
      field("pattern", $._pattern),
      optional(seq("when", field("guard", $._expression))),
      "do",
      optional(field("body", $.block)),
    ),

    try_expression: ($) => seq(
      "try",
      field("protected", $.block),
      repeat1($.catch_clause),
      "end",
    ),

    catch_clause: ($) => seq(
      "catch",
      field("pattern", $._pattern),
      optional(seq("when", field("guard", $._expression))),
      "do",
      optional(field("body", $.block)),
    ),

    raise_expression: ($) => seq(
      "raise",
      field("value", $._expression),
    ),

    _pattern: ($) => choice(
      $.wildcard_pattern,
      $.identifier,
      $.integer,
      $.float,
      $.string,
      $.boolean,
      $.nil,
      $.list_pattern,
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
      "=",
      field("pattern", $._pattern),
    ),

    rest_pattern: ($) => seq(
      "..",
      optional(field("name", choice($.wildcard_pattern, $.identifier))),
    ),

    boolean: (_) => choice("true", "false"),
    nil: (_) => "nil",

    float: (_) => token(choice(
      /[0-9]+\.[0-9]+([eE][+-]?[0-9]+)?/,
      /[0-9]+[eE][+-]?[0-9]+/,
    )),

    integer: (_) => token(/[0-9]+/),

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
