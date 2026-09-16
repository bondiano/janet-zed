# Prints the type skeleton of Janet's core: `server/src/analysis/types/core.d.janet`.
#
# One declaration per binding of `root-env`, plus the special forms and the PEG specials, which
# no environment holds. Every type is `:any`: the skeleton carries names, arity, markers and
# docs, and later phases replace the `:any`s. Regenerate after a Janet upgrade:
#
#   janet scripts/core-skeleton.janet > server/src/analysis/types/core.d.janet

(def- skeleton/markers {'& true '&opt true '&keys true '&named true})

(defn- skeleton/escape [text]
  (def out @"")
  (each byte text
    (case byte
      92 (buffer/push-string out `\\`)
      34 (buffer/push-string out `\"`)
      10 (buffer/push-string out `\n`)
      9 (buffer/push-string out `\t`)
      13 (buffer/push-string out `\r`)
      (buffer/push-byte out byte)))
  (string out))

# The first docstring line when it reads like a call: `(map f ind & inds)`.
(defn- skeleton/signature [binding]
  (when-let [doc (binding :doc)
             line (string/trim (first (string/split "\n" doc)))]
    (when (and (string/has-prefix? "(" line) (string/has-suffix? ")" line)) line)))

# `(map f ind & inds)` -> `f ind & inds`, as written: markers and destructuring included.
(defn- skeleton/parameters [name line]
  (if (= line (string "(" name ")"))
    ""
    (string/slice line (+ 2 (length name)) -2)))

# One type per parameter; markers take none of their own.
(defn- skeleton/arity [line]
  (count |(not (and (symbol? $) (skeleton/markers $))) (slice (parse line) 1)))

# What a predicate says its first argument is wherever it answers truly, for narrowing inside
# the branch it guards. A predicate that tests a value rather than a type — `odd?`, `empty?`,
# `true?` — narrows `:any`: it is marked as looked at, and tells a branch nothing.
(def- skeleton/narrows
  {'abstract? ":abstract"
   'array? ":array"
   'boolean? ":boolean"
   'buffer? ":buffer"
   'bytes? "(or :string :buffer :symbol :keyword)"
   'cfunction? ":cfunction"
   'dictionary? "(or :struct :table)"
   'fiber? ":fiber"
   'function? ":function"
   'indexed? "(or :tuple :array)"
   'keyword? ":keyword"
   'nil? ":nil"
   'number? ":number"
   'string? ":string"
   'struct? ":struct"
   'symbol? ":symbol"
   'table? ":table"
   'tuple? ":tuple"})

# Every name ending in `?` carries a mark, so that a new predicate cannot arrive unconsidered.
(defn- skeleton/narrowing [name]
  (if (string/has-suffix? "?" (string name))
    (string " :narrows " (get skeleton/narrows name ":any"))
    ""))

(defn- skeleton/callable [definer name line doc &opt narrows]
  (def types (string/join (map (fn [_] ":any") (range (skeleton/arity line))) " "))
  (printf "(%s %s\n  {:params [%s] :ret :any%s}\n  \"%s\"\n  [%s])\n"
          definer name types (or narrows "") (skeleton/escape doc)
          (skeleton/parameters name line)))

(defn- skeleton/value [definer name doc]
  (printf "(%s %s\n  {:type :any}\n  \"%s\"\n  nil)\n" definer name (skeleton/escape doc)))

# Documented as their signature alone: the prose of these lives in the server, which is where it
# is read from.
(defn- skeleton/form [line]
  (skeleton/callable "defn" (first (parse line)) line line))

# `SPECIAL_FORMS` of `server/src/analysis/stdlib.rs`.
(def- skeleton/special-forms
  ["(break &opt value)"
   "(def name meta... value)"
   "(do & body)"
   "(fn name? [params] & body)"
   "(if condition when-true &opt when-false)"
   "(quasiquote x)"
   "(quote x)"
   "(set place value)"
   "(splice x)"
   "(unquote x)"
   "(upscope & body)"
   "(var name meta... value)"
   "(while condition & body)"])

# `SPECIALS` then `ALIASES` of `server/src/analysis/peg.rs`.
(def- skeleton/peg-specials
  ["(sequence & patts)"
   "(choice & patts)"
   "(any patt)"
   "(some patt)"
   "(opt patt)"
   "(between min max patt)"
   "(at-least n patt)"
   "(at-most n patt)"
   "(repeat n patt)"
   "(range & ranges)"
   "(set chars)"
   "(look ?offset patt)"
   "(not patt)"
   "(if cond patt)"
   "(if-not cond patt)"
   "(to patt)"
   "(thru patt)"
   "(til sep patt)"
   "(sub window patt)"
   "(split sep patt)"
   "(backmatch ?tag)"
   "(lenprefix n patt)"
   "(drop patt)"
   "(only-tags patt)"
   "(error ?patt)"
   "(capture patt ?tag)"
   "(accumulate patt ?tag)"
   "(group patt ?tag)"
   "(replace patt subst ?tag)"
   "(cmt patt fun ?tag)"
   "(cms patt fun ?tag)"
   "(constant value ?tag)"
   "(argument n ?tag)"
   "(position ?tag)"
   "(line ?tag)"
   "(column ?tag)"
   "(backref prev-tag ?tag)"
   "(unref patt ?tag)"
   "(nth index patt ?tag)"
   "(number patt ?base ?tag)"
   "(int width ?tag)"
   "(int-be width ?tag)"
   "(uint width ?tag)"
   "(uint-be width ?tag)"
   "(debug)"
   "(! patt)"
   "($ ?tag)"
   "(% patt ?tag)"
   "(* & patts)"
   "(+ & patts)"
   "(-> prev-tag ?tag)"
   "(/ patt subst ?tag)"
   "(<- patt ?tag)"
   "(> ?offset patt)"
   "(? patt)"
   "(??)"
   "(quote patt ?tag)"])

(def- skeleton/bindings @[])
(eachp [name binding] root-env
  (when (and (symbol? name) (table? binding) (not (binding :private)))
    (array/push skeleton/bindings [name binding])))
(sort-by (fn [[name]] (string name)) skeleton/bindings)

(printf "# Types of Janet's core, from Janet %s." janet/version)
(print "#")
(print "# `scripts/core-skeleton.janet` writes the skeleton — every name with its arity and its docs —")
(print "# and `scripts/core-ctypes.janet` fills in what Janet's C sources answer for. The rest is written")
(print "# here by hand, so a Janet upgrade adds and removes entries rather than regenerating the file;")
(print "# the test `core_declares_every_binding_of_the_installed_janet` names what changed.")
(print "#")
(print "# An `:any` left standing is a position where any value belongs: the argument of a predicate, the")
(print "# values `print` and `array/push` take, the form a macro is handed. A `# TODO` marks the other")
(print "# kind — a position whose type nobody has worked out yet.")
(print)
(each [name binding] skeleton/bindings
  (def line (skeleton/signature binding))
  (def doc (binding :doc))
  (cond
    line (skeleton/callable (if (binding :macro) "defmacro" "defn") name line doc
                            (skeleton/narrowing name))
    (binding :ref) (skeleton/value "var" name doc)
    (skeleton/value "def" name doc)))

(print "# What a PEG pattern is, for the specials below and for `peg/*` above: a name rather than")
(print "# `:any`, since `true` and a function are the values a pattern is not.")
(print)
(print "(def Pattern :typedef")
(print "  (or :string :buffer :number :keyword :tuple :array :struct :table :abstract))")
(print)
(print "# What `ffi/*` calls a type: a keyword like `:int`, a struct of them, or what `ffi/struct` made.")
(print)
(print "(def CType :typedef (or :keyword :symbol :tuple :array :struct :abstract))")
(print)
(print "# Special forms: Janet compiles them, so no environment holds them.")
(print)
(each line skeleton/special-forms (skeleton/form line))

(print "# PEG specials: they name patterns rather than bindings, and share names with the core")
(print "# above, so they declare themselves in a block of their own.")
(print "(comment :peg")
(print)
(each line skeleton/peg-specials (skeleton/form line))
(print ")")
