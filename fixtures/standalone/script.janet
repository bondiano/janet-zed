#!/usr/bin/env janet
# Standalone script, no project.janet around it: highlighting and REPL eval.

(defn fib
  "Naive Fibonacci."
  [n]
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(def greeting ``
Long string
  with "quotes" and \no escapes
``)

(def literals
  {:string "tab\tnewline\n"
   :buffer @"mutable"
   :numbers [1 -2.5 0xff 1e3 16r1f]
   :char (chr "a")
   :constants [true false nil]
   :symbol 'quoted
   :tuple '(1 2 3)
   :array @[1 2 3]
   :struct {:a 1}
   :table @{:b 2}})

(defn main [& args]
  (print greeting)
  (printf "fib(20) = %d" (fib 20))
  (pp literals))

# Evaluate these one by one from the editor.
(comment
  (fib 10)
  (-> 5 (+ 1) (* 2))
  (map fib (range 10)))
