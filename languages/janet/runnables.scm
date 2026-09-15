; (defn main [& args] ...) → run the file
((source
  (par_tup_lit
    .
    (sym_lit) @_defn
    .
    (sym_lit) @run
    (#match? @_defn "^defn-?$")
    (#eq? @run "main")))
  (#set! tag janet-main))

; (declare-project ...) in project.janet → jpm tasks
((source
  (par_tup_lit
    .
    (sym_lit) @run
    (#eq? @run "declare-project")))
  (#set! tag janet-project))
