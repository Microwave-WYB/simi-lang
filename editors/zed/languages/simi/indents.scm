(block_expression
  "end" @end) @indent

(if_expression
  "end" @end) @indent

(case_expression
  "end" @end) @indent

(protected_expression
  "end" @end) @indent

(case_clause) @indent
(catch_arm) @indent

(elseif_clause) @indent
(else_clause) @indent

(parameters
  ")" @end) @indent

(declared_parameters
  ")" @end) @indent

(arguments
  ")" @end) @indent

(list
  "]" @end) @indent

(map
  "}" @end) @indent

(list_pattern
  "]" @end) @indent

(bytes_pattern
  "]" @end) @indent

(map_pattern
  "}" @end) @indent
