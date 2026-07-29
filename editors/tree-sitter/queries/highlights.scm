(comment) @comment

(string) @string
(escape_sequence) @string.escape
(integer) @number
(float) @number
(boolean) @constant.builtin
(nil) @constant.builtin
(wildcard_pattern) @variable.builtin

[
  "fn"
  "do"
  "end"
  "let"
  "alias"
  "requires"
  "if"
  "then"
  "elseif"
  "else"
  "case"
  "of"
  "when"
  "raise"
  "panic"
  "todo"
  "catch"
  "tap"
] @keyword

[
  "and"
  "or"
  "not"
] @operator

[
  "="
  "=="
  "!="
  "!"
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
  "<>"
  "?"
  "?>"
  "|>"
  "<|"
  ".."
  "->"
  "=>"
  "|"
] @operator

(function_declaration
  name: (identifier) @function)

(alias_declaration
  name: (identifier) @type.definition)

(type_declaration
  keyword: (identifier) @keyword
  name: (identifier) @type.definition)

(named_type
  name: (identifier) @type)

(type_variable) @type.parameter

(callable_type_parameter
  label: (identifier) @variable.parameter)

(parameter
  (identifier) @variable.parameter)

(declared_parameter
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

(map_shorthand
  name: (identifier) @variable)

(map_pattern_field
  name: (identifier) @property)

(bytes_pattern_fixed_capture
  name: (identifier) @variable)

(rest_pattern
  name: (identifier) @variable)

["#" "(" ")" "[" "]" "{" "}"] @punctuation.bracket
["," "."] @punctuation.delimiter
