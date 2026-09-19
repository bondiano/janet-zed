# Debugger end-to-end tests: pausing a program that never waits.

(defn spin []
  (var i 0)
  (while true (++ i)))

(spin)
