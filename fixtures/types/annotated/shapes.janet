# Tagged unions of structs: typedefs, constructors with written signatures, and every way Janet
# tells the members apart — `case`, `match`, `cond`, `=` on the tag, and predicates.

(def Circle :typedef {:kind :circle :r :number})
(def Rect :typedef {:kind :rect :w :number :h :number})
(def Label :typedef {:kind :label :text :string :size :number?})
(def Shape :typedef (or Circle Rect Label))
(def Point :typedef [:number :number])

(defn circle
  {:params [:number] :ret Circle}
  "A circle of radius `r`."
  [r]
  {:kind :circle :r r})

(defn rect
  {:params [:number :number] :ret Rect}
  "A `w` by `h` rectangle."
  [w h]
  {:kind :rect :w w :h h})

(defn caption
  {:params [:string :number?] :ret Label}
  "Text, at a size or the default one."
  [text &opt size]
  {:kind :label :text text :size size})

(defn area
  {:params [Shape] :ret :number}
  "The area a shape covers; a label covers none."
  [shape]
  (case (shape :kind)
    :circle (* math/pi (shape :r) (shape :r))
    :rect (* (shape :w) (shape :h))
    :label 0))

(defn describe-shape
  {:params [Shape] :ret :string}
  "One line about a shape."
  [shape]
  (match shape
    {:kind :circle :r r} (string/format "circle r=%g" r)
    {:kind :rect :w w :h h} (string/format "rect %gx%g" w h)
    {:kind :label :text text} (string "label " text)))

(defn font-size
  {:params [Shape] :ret :number}
  "The size text on the shape is drawn at."
  [shape]
  (cond
    (= (shape :kind) :label) (or (shape :size) 12)
    (= (shape :kind) :rect) (min (shape :w) (shape :h))
    10))

(defn widest
  {:params [[Shape]] :ret Shape?}
  "The shape of the largest area, or nil for none."
  [shapes]
  (var best nil)
  (each shape shapes
    (when (or (nil? best) (> (area shape) (area best)))
      (set best shape)))
  best)

(defn centre
  {:params [Point Point] :ret Point}
  "The point halfway between two."
  [[x1 y1] [x2 y2]]
  [(/ (+ x1 x2) 2) (/ (+ y1 y2) 2)])

(defn scale
  {:params [Shape :number] :ret Shape}
  "The shape grown by `k`."
  [shape k]
  (case (shape :kind)
    :circle (circle (* k (shape :r)))
    :rect (rect (* k (shape :w)) (* k (shape :h)))
    (caption (shape :text) (when-let [size (shape :size)] (* k size)))))

(def shapes
  {:type [Shape]}
  [(circle 1) (rect 2 3) (caption "hi") (caption "big" 20)])

(def total {:type :number} (sum (map area shapes)))

(assert (= 4 (length shapes)))
(assert (> total 9))
(assert (= "rect 2x3" (describe-shape (rect 2 3))))
(assert (= 20 (font-size (caption "big" 20))))
(assert (= 12 (font-size (caption "small"))))
(assert (= :rect ((widest shapes) :kind)))
(assert (nil? (widest [])))
(assert (deep= [1 1] (centre [0 0] [2 2])))
(assert (= 24 (area (scale (rect 2 3) 2))))
(assert (= "label hi" (describe-shape (scale (caption "hi") 3))))
