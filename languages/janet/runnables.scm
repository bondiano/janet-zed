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

; (start-suite ...) in a spork/test file → run the file
((source
  (par_tup_lit
    .
    (sym_lit) @run
    (#eq? @run "start-suite")))
  (#set! tag janet-main))

; (deftest name ...) and (deftest: type name args ...) → judge
((source
  (par_tup_lit
    .
    (sym_lit) @run
    (#match? @run "^deftest:?$")))
  (#set! tag janet-judge))

; (declare-project ...) in project.janet → jpm tasks
((source
  (par_tup_lit
    .
    (sym_lit) @run
    (#eq? @run "declare-project")))
  (#set! tag janet-project))
