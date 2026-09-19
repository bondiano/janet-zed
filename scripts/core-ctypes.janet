# Fills the types of Janet's C functions in `janet-check/src/analysis/types/core.d.janet` from the C
# sources those functions are written in.
#
# A C function says what it takes by the accessor it reads each argument with — `janet_getstring`
# takes a string, `janet_optnat` an optional number — and says what it gives by the
# `janet_wrap_*` it returns. Where the accessors disagree, or the returns do, the position keeps
# its `:any` and the entry gains a `# TODO` naming what was left unclear.
#
# Only `:any` positions are written, so types written by hand survive a rerun:
#
#   janet scripts/core-ctypes.janet ../janet janet-check/src/analysis/types/core.d.janet
#
# It rewrites the file in place.

(def- usage "usage: janet scripts/core-ctypes.janet <janet-checkout> <core.d.janet>")

# What each argument accessor of `janet.h` says its argument is. An accessor missing here says
# something we have no type for, and leaves its argument alone.
(def- ctypes/argument
  {"abstract" ":abstract"
   "array" ":array"
   "boolean" ":boolean"
   "buffer" ":buffer"
   "bytes" "(or :string :buffer :symbol :keyword)"
   "cbytes" "(or :string :buffer :symbol :keyword)"
   "cfunction" ":cfunction"
   "channel" ":abstract"
   "cstring" ":string"
   "dictionary" "(or :struct :table)"
   "fiber" ":fiber"
   "file" ":abstract"
   "flags" ":keyword"
   "function" ":function"
   "halfrange" ":number"
   "index" ":number"
   "indexed" "(or :tuple :array)"
   "integer" ":number"
   "integer16" ":number"
   "integer64" ":number"
   "jfile" ":abstract"
   "jstream" ":abstract"
   "keyword" ":keyword"
   "nat" ":number"
   "number" ":number"
   "pointer" ":pointer"
   "size" ":number"
   "string" ":string"
   "struct" ":struct"
   "symbol" ":symbol"
   "table" ":table"
   "uinteger" ":number"
   "uinteger16" ":number"
   "uinteger64" ":number"})

(def- ctypes/wrapped
  {"janet_wrap_abstract" ":abstract"
   "janet_wrap_array" ":array"
   "janet_wrap_boolean" ":boolean"
   "janet_wrap_buffer" ":buffer"
   "janet_wrap_cfunction" ":cfunction"
   "janet_wrap_false" ":boolean"
   "janet_wrap_fiber" ":fiber"
   "janet_wrap_function" ":function"
   "janet_wrap_integer" ":number"
   "janet_wrap_keyword" ":keyword"
   "janet_wrap_nil" ":nil"
   "janet_wrap_number" ":number"
   "janet_wrap_pointer" ":pointer"
   "janet_wrap_string" ":string"
   "janet_wrap_struct" ":struct"
   "janet_wrap_symbol" ":symbol"
   "janet_wrap_table" ":table"
   "janet_wrap_true" ":boolean"
   "janet_wrap_tuple" ":tuple"
   "janet_ckeywordv" ":keyword"
   "janet_cstringv" ":string"
   "janet_csymbolv" ":symbol"
   "janet_stringv" ":string"})

## The C sources

# `JANET_CORE_FN(cfun_string_slice, "(string/slice bytes &opt start end)", "…") { … }`. A core C
# function ends at the closing brace in the first column, as every one of them is written.
(def- ctypes/core-fn
  (peg/compile
    ~{:literal (* `"` (any (+ (* `\` 1) (if-not `"` 1))) `"`)
      :signature (* :s* (<- :literal))
      :body (<- (any (if-not "\n}" 1)))
      :main (any (+ (* "JANET_CORE_FN(" :s* (some (+ :w "_")) "," :signature
                       "," (thru ") {") :body)
                    1))}))

# The name `"(string/slice bytes &opt start end)"` registers, as a C string literal.
(defn- ctypes/called [literal]
  (def line (try (string/trim (eval-string literal)) ([_] "")))
  (when (string/has-prefix? "(" line)
    (first (string/split " " (string/slice line 1 (if (string/has-suffix? ")" line) -2 -1))))))

(defn- ctypes/bodies [root]
  (def bodies @{})
  (each name (sorted (os/dir (string root "/src/core")))
    (when (string/has-suffix? ".c" name)
      (each [literal body] (partition 2 (peg/match ctypes/core-fn
                                                   (slurp (string root "/src/core/" name))))
        (when-let [called (ctypes/called literal)]
          (put bodies called body)))))
  bodies)

## What a body says

(def- ctypes/gets
  (peg/compile ~(any (+ (* "janet_get" (<- (some (+ :w "_"))) "(argv,"
                           (/ (<- (some :d)) ,scan-number))
                        1))))

(def- ctypes/opts
  (peg/compile ~(any (+ (* "janet_opt" (<- (some (+ :w "_"))) "(argv,argc,"
                           (/ (<- (some :d)) ,scan-number))
                        1))))

(def- ctypes/ranges
  (peg/compile ~(any (+ (* "janet_get" (+ "start" "end") "range(argv,argc,"
                           (/ (<- (some :d)) ,scan-number))
                        1))))

# Argument index to the type its accessor gives it; `:ambiguous` where two accessors disagree,
# which is a function that takes either and a type we cannot write.
(defn- ctypes/arguments [body]
  (def flat (string/replace-all " " "" (string/replace-all "\n" "" body)))
  (def found @{})
  (defn note [index written]
    (cond
      (nil? (found index)) (put found index written)
      (not= (found index) written) (put found index :ambiguous)))
  (each [accessor index] (partition 2 (peg/match ctypes/gets flat))
    (when-let [ty (get ctypes/argument accessor)] (note index ty)))
  (each [accessor index] (partition 2 (peg/match ctypes/opts flat))
    (when-let [ty (get ctypes/argument accessor)]
      (note index (if (string/has-prefix? ":" ty) (string ty "?") ty))))
  (each index (peg/match ctypes/ranges flat) (note index ":number?"))
  # `janet_getslice(argc, argv)` reads the two optional bounds after the first argument.
  (when (string/find "janet_getslice(argc,argv)" flat)
    (note 1 ":number?")
    (note 2 ":number?"))
  found)

# What every `return` of a body wraps. Nothing recognised in a return — a variable, a call we do
# not know — means the whole answer is unknown.
(defn- ctypes/returns [body arguments]
  (def wrapped @{})
  (each at (string/find-all "return " body)
    (def statement (string/slice body at (or (string/find ";" body at) -1)))
    (var known false)
    (eachp [call ty] ctypes/wrapped
      (when (string/find (string call "(") statement)
        (set known true)
        (put wrapped ty true)))
    (unless known
      (if (string/find "argv[0]" statement)
        (put wrapped (or (get arguments 0) :ambiguous) true)
        (put wrapped :ambiguous true))))
  (def types (sorted (keys wrapped)))
  (cond
    (or (empty? types) (index-of :ambiguous types)) nil
    (= 1 (length types)) (first types)
    (and (= 2 (length types)) (index-of ":nil" types))
    (let [other (first (filter |(not= $ ":nil") types))]
      (when (string/has-prefix? ":" other) (string other "?")))))

## The file

# A type as it is written in source: `%j` turns `[:a]` into `(:a)`, which is a different type.
(defn- ctypes/write [ty]
  (defn items [ts] (string/join (map ctypes/write ts) " "))
  (defn pairs- [ds] (string/join (map |(string (ctypes/write ($ 0)) " " (ctypes/write ($ 1)))
                                      (sorted-by |(string ($ 0)) (pairs ds))) " "))
  (cond
    (keyword? ty) (string ":" ty)
    (symbol? ty) (string ty)
    (array? ty) (string "@[" (items ty) "]")
    (tuple? ty) (if (= :brackets (tuple/type ty))
                  (string "[" (items ty) "]")
                  (string "(" (items ty) ")"))
    (table? ty) (string "@{" (pairs- ty) "}")
    (struct? ty) (string "{" (pairs- ty) "}")
    (string/format "%j" ty)))

# `[bytes &opt start end]` -> the type slot each argument index falls in. `&` and its kin cover
# every index from their own with one type, so one of those disagreeing is unknowable.
(defn- ctypes/slots [vector]
  (def slots @{})
  (var slot 0)
  (var rest nil)
  (each param vector
    (case param
      '&opt nil
      '& (set rest slot)
      '&keys (set rest slot)
      '&named (set rest slot)
      (do (put slots (length slots) slot) (++ slot))))
  [slots rest])

# The metadata line of one entry, with the `:any`s the C source can answer for filled in.
(defn- ctypes/filled [meta vector body]
  (def arguments (ctypes/arguments body))
  (def [slots rest] (ctypes/slots vector))
  (def params (map ctypes/write (get meta :params [])))
  (def unclear @[])
  (each index (sorted (keys arguments))
    (def slot (get slots index))
    (def ty (get arguments index))
    (when (and slot (= :any (get (meta :params) slot)) (or (nil? rest) (< slot rest)))
      (if (= ty :ambiguous)
        (array/push unclear (string "what " (get vector index) " is"))
        (put params slot ty))))
  (def ret
    (if (= :any (get meta :ret))
      (or (ctypes/returns body arguments)
          (do (array/push unclear "what it returns") ":any"))
      (ctypes/write (get meta :ret))))
  (string "  {:params [" (string/join params " ") "] :ret " ret
          (if-let [narrows (get meta :narrows)] (string " :narrows " (ctypes/write narrows)) "")
          (if-let [throws (get meta :throws)] (string " :throws " (ctypes/write throws)) "")
          "}"
          (if (empty? unclear) "" (string " # TODO: the C source does not say "
                                          (string/join unclear ", ")))))

(defn main [_ &opt root file]
  (unless (and root file) (eprint usage) (os/exit 1))
  (def bodies (ctypes/bodies root))
  (def lines (string/split "\n" (slurp file)))
  (def out @[])
  (var peg false)
  (var index 0)
  (while (< index (length lines))
    (def line (lines index))
    (when (string/has-prefix? "(comment :peg" line) (set peg true))
    (def entry
      (when (and (not peg)
                 (or (string/has-prefix? "(defn " line) (string/has-prefix? "(defmacro " line))
                 (< (+ index 3) (length lines)))
        (try (parse (string/join (slice lines index (+ index 4)) "\n")) ([_] nil))))
    (def body (and entry (get bodies (string (entry 1)))))
    (cond
      (nil? body) (do (array/push out line) (++ index))
      (do
        (array/push out (lines index))
        (array/push out (ctypes/filled (entry 2) (entry 4) body))
        (array/push out ;(slice lines (+ index 2) (+ index 4)))
        (+= index 4))))
  (spit file (string/join out "\n")))
