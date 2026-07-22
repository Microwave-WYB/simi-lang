; This query targets the node contract documented in README.md. Keep it in
; sync with editors/tree-sitter when the shared grammar changes.

(comment) @comment

(string) @string
(escape_sequence) @string.escape

[
  (integer)
  (float)
] @number

[
  "true"
  "false"
] @boolean

"nil" @constant.builtin

[
  "fn"
  "do"
  "end"
  "if"
  "then"
  "elseif"
  "else"
  "let"
  "tap"
  "loop"
  "break"
  "continue"
  "match"
  "with"
  "case"
  "when"
  "raise"
  "try"
  "catch"
] @keyword

[
  "and"
  "or"
  "not"
] @keyword

[
  "+"
  "-"
  "*"
  "/"
  "//"
  "%"
  "=="
  "!="
  "<"
  "<="
  ">"
  ">="
  "="
  "|>"
  "<|"
  "->"
  ".."
] @operator

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

[
  ","
  "."
] @punctuation.delimiter

(identifier) @variable

(parameters
  (identifier) @variable.parameter)

(function_declaration
  name: (identifier) @function)

(call_expression
  function: (identifier) @function)

(field_expression
  field: (identifier) @property)

(map_entry
  key: (identifier) @property)

(map_pattern_field
  name: (identifier) @property)
