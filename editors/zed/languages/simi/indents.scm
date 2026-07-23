(function_declaration
  "end" @end) @indent

(function_expression
  "end" @end) @indent

(block_expression
  "end" @end) @indent

(if_expression
  "end" @end) @indent

(loop_expression
  "end" @end) @indent

; Clauses indent their own bodies. The enclosing case must not add another
; level, so `of` and the final `end` align with `case`.
(case_expression
  "end" @end)

(try_expression
  "end" @end) @indent

(case_clause
  "of" @end) @indent

(catch_clause
  "catch" @end) @indent

(elseif_clause) @indent
(else_clause) @indent

(parameters
  ")" @end) @indent

(arguments
  ")" @end) @indent

(list
  "]" @end) @indent

(map
  "}" @end) @indent

(list_pattern
  "]" @end) @indent

(map_pattern
  "}" @end) @indent
