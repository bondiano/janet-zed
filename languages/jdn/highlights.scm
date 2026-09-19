; Janet data: literals only, no calls or builtins.

(sym_lit) @variable

(kwd_lit) @string.special.symbol

[
  (str_lit)
  (long_str_lit)
  (buf_lit)
  (long_buf_lit)
] @string

(num_lit) @number

(bool_lit) @boolean

(nil_lit) @constant.builtin

(comment) @comment

[
  "{" "@{" "}"
  "[" "@[" "]"
  "(" "@(" ")"
] @punctuation.bracket
