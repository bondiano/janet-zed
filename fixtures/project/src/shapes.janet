# Geometry primitives: hover, completion, go-to-def, documentSymbol.

(def pi-ish
  "Close enough for fixtures."
  3.14159)

(var created 0)

(defn- bump []
  (++ created))

(defn circle
  "Make a circle with radius `r`."
  [r]
  (bump)
  {:kind :circle :r r})

(defn rect
  "Make a rectangle `w` by `h`."
  [w h]
  (bump)
  {:kind :rect :w w :h h})

(defn area
  "Area of any shape."
  [shape]
  (case (shape :kind)
    :circle (* pi-ish (shape :r) (shape :r))
    :rect (* (shape :w) (shape :h))
    (errorf "unknown shape: %q" shape)))

(defmacro timed
  "Evaluate `body`, return `[result seconds]`."
  [& body]
  (with-syms [start res]
    ~(let [,start (os/clock)
           ,res (do ,;body)]
       [,res (- (os/clock) ,start)])))
