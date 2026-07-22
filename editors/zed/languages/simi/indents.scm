; Keyword-delimited expression blocks.
(function_declaration
  "end" @end) @indent

(function_expression
  "end" @end) @indent

(if_expression
  "end" @end) @indent

(loop_expression
  "end" @end) @indent

(match_expression
  "end" @end) @indent

(try_expression
  "end" @end) @indent

; Case bodies start after their arrow and end at the next case or parent end.
(match_case) @indent

; Delimiter-based forms, including calls, lists, maps, and computed keys.
(_ "(" ")" @end) @indent
(_ "[" "]" @end) @indent
(_ "{" "}" @end) @indent
