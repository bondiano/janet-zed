((comment) @injection.content
  (#set! injection.language "comment"))

; Docstrings are Markdown. Only "…" ones: Zed has no #offset!, so a `…` long string would reach the
; Markdown parser with its backticks and read as one code span.
((par_tup_lit
  .
  (sym_lit) @_def
  .
  (sym_lit)
  .
  (kwd_lit)*
  .
  (str_lit) @injection.content
  .
  (_))
  (#match? @_def "^(def|def-|var|var-|defn|defn-|defmacro|defmacro-|varfn|defglobal|varglobal)$")
  (#set! injection.language "markdown"))

((par_tup_lit
  .
  (sym_lit) @_def
  .
  (sym_lit)
  .
  (str_lit) @injection.content
  .)
  (#eq? @_def "defdyn")
  (#set! injection.language "markdown"))
