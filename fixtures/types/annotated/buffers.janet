# Files read into buffers and handed to functions that take text: `slurp`, `file/read`, buffers
# built up with `buffer/push`, and strings and buffers used where either will do.

(def Text :typedef '(or :string :buffer))
(def Line :typedef {:number :number :text :string})

(defn lines
  {:params [Text] :ret @[:string]}
  "The lines of a text, without their ends."
  [text]
  (def out @[])
  (each line (string/split "\n" text)
    (unless (empty? line) (array/push out (string line))))
  out)

(defn numbered
  {:params [Text] :ret @[Line]}
  "Each non-empty line with its number."
  [text]
  (def out @[])
  (eachp [i line] (lines text)
    (array/push out {:number (inc i) :text line}))
  out)

(defn shout
  {:params [:string] :ret :string}
  "The text in capitals."
  [s]
  (string/ascii-upper s))

(defn render
  {:params [@[Line]] :ret :buffer}
  "The lines numbered, into a buffer."
  [ls]
  (def buf @"")
  (each {:number n :text t} ls
    (buffer/push buf (string n) ": " t "\n"))
  buf)

(defn word-count
  {:params [Text] :ret :number}
  "How many words a text holds."
  [text]
  (length (filter |(not (empty? $)) (string/split " " (string/replace-all "\n" " " text)))))

(def tmp (or (os/getenv "TMPDIR") (os/getenv "TEMP") "."))
(def path (string tmp "/annotated-buffers-" (os/getpid) ".tmp"))
(spit path "alpha beta\n\ngamma\n")

(defer (os/rm path)
  (def contents (slurp path))
  (assert (buffer? contents))
  (assert (= 2 (length (lines contents))))
  (assert (= 3 (word-count contents)))
  (assert (= "ALPHA BETA" (shout (first (lines contents)))))

  (def from-file
    (with [f (file/open path :r)]
      (file/read f :all)))
  (assert (= 3 (word-count from-file)))

  (def first-line
    (with [f (file/open path :r)]
      (file/read f :line)))
  (assert (= "alpha beta\n" (string first-line)))

  (def out (render (numbered contents)))
  (buffer/push-string out "end")
  (assert (string/has-suffix? "end" out))
  (assert (= "1: alpha beta\n2: gamma\nend" (string out))))

(def mixed @"abc")
(buffer/push mixed "def" (string/repeat "g" 2))
(assert (= "ABCDEFGG" (shout (string mixed))))
(assert (= 1 (word-count (string/join ["one" (string @"")] ""))))
(assert (deep= @["x" "y"] (lines (buffer "x\ny"))))
