(function_declaration
  "end" @end) @indent

(function_expression
  "end" @end) @indent

(if_expression
  "end" @end) @indent

(loop_expression
  "end" @end) @indent

(case_expression
  "end" @end) @indent

(try_expression
  "end" @end) @indent

(pattern_clause
  "end" @end) @indent
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
