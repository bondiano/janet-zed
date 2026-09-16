# Geometry primitives: hover, completion, go-to-def, documentSymbol.

(def pi-ish
  "Close enough for fixtures."
  3.14159)

(var created 0)

(def Circle :typedef {:kind :circle :r :number})
(def Rect :typedef {:kind :rect :w :number :h :number})
(def Shape :typedef (or Circle Rect))

(defn- bump []
  (++ created))

(defn circle
  {:params [:number] :ret Circle}
  "Make a circle with radius `r`."
  [r]
  (bump)
  {:kind :circle :r r})

(defn rect
  {:params [:number :number] :ret Rect}
  "Make a rectangle `w` by `h`."
  [w h]
  (bump)
  {:kind :rect :w w :h h})

(defn area
  {:params [Shape] :ret :number :throws [:string]}
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
