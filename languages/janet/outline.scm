; Top-level definitions, and those one level inside a top-level `(comment …)`, `(upscope …)`,
; `(compwhen …)` and the like. Local `def`s inside bodies would clutter the outline.
(source
  (par_tup_lit
    .
    (sym_lit) @context
    .
    (sym_lit) @name
    (#match? @context
      "^(def|def-|defn|defn-|defmacro|defmacro-|var|var-|varfn|defglobal|varglobal|defdyn)$")) @item)

(source
  (par_tup_lit
    .
    (sym_lit) @_wrapper
    (par_tup_lit
      .
      (sym_lit) @context
      .
      (sym_lit) @name
      (#match? @context
        "^(def|def-|defn|defn-|defmacro|defmacro-|var|var-|varfn|defglobal|varglobal|defdyn)$")) @item
    (#match? @_wrapper "^(comment|do|upscope|compwhen|compif|when|unless|if|if-not|with-dyns)$")))

; judge: (deftest "name" …)
(source
  (par_tup_lit
    .
    (sym_lit) @context
    .
    [(str_lit) (sym_lit)] @name
    (#eq? @context "deftest")) @item)
