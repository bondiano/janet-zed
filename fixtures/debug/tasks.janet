# Debugger end-to-end tests: a breakpoint in a task stops the task, not the program.

(defn work [x]
  (def doubled (* x 2))
  (print "worked " doubled))

(def done (ev/chan))
(ev/spawn (work 5) (ev/give done true))
(ev/take done)
(print "done")
