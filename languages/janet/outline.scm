; Top-level definitions only: local `def`s inside bodies would clutter the outline.
(source
  (par_tup_lit
    .
    (sym_lit) @context
    .
    (sym_lit) @name
    (#match? @context
      "^(def|def-|defn|defn-|defmacro|defmacro-|var|var-|varfn|defglobal|varglobal|defdyn)$")) @item)
