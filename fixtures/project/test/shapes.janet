(import ../src/shapes)
(import ../src/report)

(def items [(shapes/rect 3 4) (shapes/circle 1)])

(assert (= 12 (shapes/area (first items))))
(assert (< 3.14 (shapes/area (last items)) 3.15))
(assert (= "2 shapes, total area 15.14, kinds: circle, rect"
           (report/summary items)))
(assert (not (first (protect (shapes/area {:kind :blob})))))

(def [value _] (shapes/timed (+ 1 2)))
(assert (= 3 value))
