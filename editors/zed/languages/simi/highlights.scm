(comment) @comment

(string) @string
(escape_sequence) @string.escape
(integer) @number
(float) @number
(boolean) @boolean
(nil) @constant.builtin
(wildcard_pattern) @variable

[
  "fn"
  "do"
  "end"
  "let"
  "alias"
  "if"
  "then"
  "after"
  "becomes"
  "elseif"
  "else"
  "loop"
  "break"
  "continue"
  "case"
  "of"
  "when"
  "raise"
  "try"
  "catch"
  "tap"
] @keyword

[
  "and"
  "or"
  "not"
  "="
  "=="
  "!="
  "+"
  "-"
  "*"
  "/"
  "//"
  "%"
  "<"
  "<="
  ">"
  ">="
  "?"
  "?>"
  "|>"
  "<|"
  ".."
  "->"
  "|"
] @operator

(identifier) @variable

(function_declaration
  name: (identifier) @function)

(alias_declaration
  name: (identifier) @type.definition)

(named_type
  name: (identifier) @type)

(type_variable) @type.parameter

(parameter
  (identifier) @variable.parameter)

(call_expression
  function: (identifier) @function)

(call_expression
  function: (field_expression
    name: (identifier) @function))

(pipeline_callee
  (identifier) @function)

(field_expression
  name: (identifier) @property)

(map_field
  name: (identifier) @property)

(map_pattern_field
  name: (identifier) @property)

["(" ")" "[" "]" "{" "}"] @punctuation.bracket
["," "."] @punctuation.delimiter
