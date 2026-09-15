# Debugger end-to-end tests: breakpoints, stepping in and out, a child fiber, an uncaught error.

(defn add [a b]
  (def sum (+ a b))
  (* sum 2))

(defn run [x]
  (def doubled (add x 1))
  (def checked (try (add doubled 0) ([err] err)))
  (print "checked " checked)
  checked)

(run 4)

(error "boom")
