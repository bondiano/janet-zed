# Cross-module usage: go-to-def into ./shapes, references/rename, threading.

(import ./shapes)

(defn total-area
  "Sum of areas."
  [items]
  (->> items
       (map shapes/area)
       (reduce + 0)))

(defn summary
  "One-line description of `items`."
  [items]
  (def by-kind (group-by |($ :kind) items))
  (string/format "%d shapes, total area %.2f, kinds: %s"
                 (length items)
                 (total-area items)
                 (string/join (map string (sorted (keys by-kind))) ", ")))

(defn render
  "Deeply nested forms for slurp/barf/raise/splice/wrap."
  [items]
  (let [lines @[]]
    (each s items
      (array/push lines (string "- " (s :kind) ": " (shapes/area s))))
    (string/join lines "\n")))
