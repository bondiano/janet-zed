; (defn name "doc"? :meta* [params] body...)
(par_tup_lit
  .
  (sym_lit) @_def
  .
  (sym_lit)
  .
  [(str_lit) (long_str_lit) (kwd_lit)]*
  .
  (sqr_tup_lit)
  .
  (_)* @function.inside
  (#match? @_def "^(defn|defn-|defmacro|defmacro-|varfn)$")) @function.around

; (fn name? [params] body...)
(par_tup_lit
  .
  (sym_lit) @_fn
  .
  (sym_lit)?
  .
  (sqr_tup_lit)
  .
  (_)* @function.inside
  (#eq? @_fn "fn")) @function.around

(short_fn_lit
  "|"
  .
  (_) @function.inside) @function.around

; Top-level forms: `[[`/`]]` jump between them, `ac` selects one.
(source
  (par_tup_lit) @class.around)

(comment)+ @comment.around
