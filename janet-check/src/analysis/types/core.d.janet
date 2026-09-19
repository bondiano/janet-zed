# Types of Janet's core, from Janet 1.41.2.
#
# `scripts/core-skeleton.janet` writes the skeleton — every name with its arity and its docs —
# and `scripts/core-ctypes.janet` fills in what Janet's C sources answer for. The rest is written
# here by hand, so a Janet upgrade adds and removes entries rather than regenerating the file;
# the test `core_declares_every_binding_of_the_installed_janet` names what changed.
#
# An `:any` left standing is a position where any value belongs: the argument of a predicate, the
# values `print` and `array/push` take, the form a macro is handed. A `# TODO` marks the other
# kind — a position whose type nobody has worked out yet.

(defn %
  {:params [:number] :ret :number}
  "(% & xs)\n\nReturns the remainder of dividing the first value of xs by each remaining value."
  [& xs])

(defmacro %=
  {:params [:any :number] :ret :number}
  "(%= x & ns)\n\nShorthand for (set x (% x n))."
  [x & ns])

(defn *
  {:params [:number] :ret :number}
  "(* & xs)\n\nReturns the product of all elements in xs. If xs is empty, returns 1."
  [& xs])

(defmacro *=
  {:params [:any :number] :ret :number}
  "(*= x & ns)\n\nShorthand for (set x (\\* x n))."
  [x & ns])

(def *args*
  {:type :keyword}
  "Dynamic bindings that will contain command line arguments at program start."
  nil)

(def *current-file*
  {:type :keyword}
  "Bound to the name of the currently compiling file."
  nil)

(def *debug*
  {:type :keyword}
  "Enables a built in debugger on errors and other useful features for debugging in a repl."
  nil)

(def *defdyn-prefix*
  {:type :keyword}
  "Optional namespace prefix to add to keywords declared with `defdyn`.\n  Use this to prevent keyword collisions between dynamic bindings."
  nil)

(def *doc-color*
  {:type :keyword}
  "Whether or not to colorize documentation printed with `doc-format`."
  nil)

(def *doc-width*
  {:type :keyword}
  "Width in columns to print documentation printed with `doc-format`."
  nil)

(def *err*
  {:type :keyword}
  "Where error printing prints output to."
  nil)

(def *err-color*
  {:type :keyword}
  "Whether or not to turn on error coloring in stacktraces and other error messages."
  nil)

(def *executable*
  {:type :keyword}
  "Name of the interpreter executable used to execute this program. Corresponds to `argv[0]` in the call to\n  `int main(int argc, char **argv);`."
  nil)

(def *exit*
  {:type :keyword}
  "When set, will cause the current context to complete. Can be set to exit from repl (or file), for example."
  nil)

(def *exit-value*
  {:type :keyword}
  "Set the return value from `run-context` upon an exit."
  nil)

(def *ffi-context*
  {:type :keyword}
  " Current native library for ffi/bind and other settings"
  nil)

(def *flychecking*
  {:type :keyword}
  "Check if the current form is being evaluated inside `flycheck`. Will be `true` while flychecking."
  nil)

(def *lint-error*
  {:type :keyword}
  "The current lint error level. The error level is the lint level at which compilation will exit with an error and not continue."
  nil)

(def *lint-levels*
  {:type :keyword}
  "A table of keyword alias to numbers denoting a lint level. Can be used to provided custom aliases for numeric lint levels."
  nil)

(def *lint-warn*
  {:type :keyword}
  "The current lint warning level. The warning level is the lint level at which and error will be printed but compilation will continue as normal."
  nil)

(def *macro-form*
  {:type :keyword}
  "Inside a macro, is bound to the source form that invoked the macro"
  nil)

(def *macro-lints*
  {:type :keyword}
  "Bound to an array of lint messages that will be reported by the compiler inside a macro.\nTo indicate an error or warning, a macro author should use `maclintf`."
  nil)

(def *module-cache*
  {:type :keyword}
  "Dynamic binding for overriding `module/cache`"
  nil)

(def *module-loaders*
  {:type :keyword}
  "Dynamic binding for overriding `module/loaders`"
  nil)

(def *module-loading*
  {:type :keyword}
  "Dynamic binding for overriding `module/loading`"
  nil)

(def *module-make-env*
  {:type :keyword}
  "Dynamic binding for creating new environments for `import`, `require`, and `dofile`. Overrides `make-env`."
  nil)

(def *module-paths*
  {:type :keyword}
  "Dynamic binding for overriding `module/paths`"
  nil)

(def *out*
  {:type :keyword}
  "Where normal print functions print output to."
  nil)

(def *peg-grammar*
  {:type :keyword}
  "The implicit base grammar used when compiling PEGs. Any undefined keywords\nfound when compiling a peg will use lookup in this table (if defined)."
  nil)

(def *pretty-format*
  {:type :keyword}
  "Format specifier for the `pp` function"
  nil)

(def *profilepath*
  {:type :keyword}
  "Path to profile file loaded when starting up the repl."
  nil)

(def *redef*
  {:type :keyword}
  "When set, allow dynamically rebinding top level defs. Will slow generated code and is intended to be used for development."
  nil)

(def *repl-prompt*
  {:type :keyword}
  "Allow setting a custom prompt at the default REPL. Not all REPLs will respect this binding."
  nil)

(def *syspath*
  {:type :keyword}
  "Path of directory to load system modules from."
  nil)

(def *task-id*
  {:type :keyword}
  "When spawning a thread or fiber, the task-id can be assigned for concurrency control."
  nil)

(defn +
  {:params [:number] :ret :number}
  "(+ & xs)\n\nReturns the sum of all xs. If xs is empty, return 0."
  [& xs])

(defmacro ++
  {:params [:any] :ret :number}
  "(++ x)\n\nIncrements the var x by 1."
  [x])

(defmacro +=
  {:params [:any :number] :ret :number}
  "(+= x & ns)\n\nIncrements the var x by n."
  [x & ns])

(defn -
  {:params [:number] :ret :number}
  "(- & xs)\n\nReturns the difference of xs. If xs is empty, returns 0. If xs has one element, returns the negative value of that element. Otherwise, returns the first element in xs minus the sum of the rest of the elements."
  [& xs])

(defmacro --
  {:params [:any] :ret :number}
  "(-- x)\n\nDecrements the var x by 1."
  [x])

(defmacro -=
  {:params [:any :number] :ret :number}
  "(-= x & ns)\n\nDecrements the var x by n."
  [x & ns])

(defmacro ->
  {:params [:any :any] :ret :any}
  "(-> x & forms)\n\nThreading macro. Inserts x as the second value in the first form\nin `forms`, and inserts the modified first form into the second form\nin the same manner, and so on. Useful for expressing pipelines of data."
  [x & forms])

(defmacro ->>
  {:params [:any :any] :ret :any}
  "(->> x & forms)\n\nThreading macro. Inserts x as the last value in the first form\nin `forms`, and inserts the modified first form into the second form\nin the same manner, and so on. Useful for expressing pipelines of data."
  [x & forms])

(defmacro -?>
  {:params [:any :any] :ret :any}
  "(-?> x & forms)\n\nShort circuit threading macro. Inserts x as the second value in the first form\nin `forms`, and inserts the modified first form into the second form\nin the same manner, and so on. The pipeline will return nil\nif an intermediate value is nil.\nUseful for expressing pipelines of data."
  [x & forms])

(defmacro -?>>
  {:params [:any :any] :ret :any}
  "(-?>> x & forms)\n\nShort circuit threading macro. Inserts x as the last value in the first form\nin `forms`, and inserts the modified first form into the second form\nin the same manner, and so on. The pipeline will return nil\nif an intermediate value is nil.\nUseful for expressing pipelines of data."
  [x & forms])

(defn /
  {:params [:number] :ret :number}
  "(/ & xs)\n\nReturns the quotient of xs. If xs is empty, returns 1. If xs has one value x, returns the reciprocal of x. Otherwise return the first value of xs repeatedly divided by the remaining values."
  [& xs])

(defmacro /=
  {:params [:any :number] :ret :number}
  "(/= x & ns)\n\nShorthand for (set x (/ x n))."
  [x & ns])

(defn <
  {:params [:number] :ret :boolean}
  "(< & xs)\n\nCheck if xs is in ascending order. Returns a boolean."
  [& xs])

(defn <=
  {:params [:number] :ret :boolean}
  "(<= & xs)\n\nCheck if xs is in non-descending order. Returns a boolean."
  [& xs])

(defn =
  {:params [:any] :ret :boolean}
  "(= & xs)\n\nCheck if all values in xs are equal. Returns a boolean."
  [& xs])

(defn >
  {:params [:number] :ret :boolean}
  "(> & xs)\n\nCheck if xs is in descending order. Returns a boolean."
  [& xs])

(defn >=
  {:params [:number] :ret :boolean}
  "(>= & xs)\n\nCheck if xs is in non-ascending order. Returns a boolean."
  [& xs])

(defn abstract?
  {:params [:any] :ret :boolean :narrows :abstract}
  "(abstract? x)\n\nCheck if x is an abstract type."
  [x])

(defn accumulate
  {:params [(fn [b a] b) b (or [a] @[a])] :ret @[b]}
  "(accumulate f init ind)\n\nSimilar to `reduce`, but accumulates intermediate values into an array.\nThe last element in the array is what would be the return value from `reduce`.\nThe `init` value is not added to the array (the return value will have the same\nnumber of elements as `ind`).\nReturns a new array."
  [f init ind])

(defn accumulate2
  {:params [(fn [a a] a) (or [a] @[a])] :ret @[a]}
  "(accumulate2 f ind)\n\nThe 2-argument version of `accumulate` that does not take an initialization value.\nThe first value in `ind` will be added to the array as is, so the length of the\nreturn value will be `(length ind)`."
  [f ind])

(defn all
  {:params [(fn [a] :any) (or [a] @[a]) (or [:any] @[:any])] :ret :boolean}
  "(all pred ind & inds)\n\nReturns true if applying `pred` to every value in a data\nstructure `ind` results in only truthy values, but only if no\n`inds` are provided. Multiple data structures can be handled\nif each `inds` is a data structure and `pred` is a function\nof arity one more than the number of `inds`. Returns the first\nfalsey result encountered. Note that `pred` is only called as\nmany times as the length of the shortest of `ind` and each of\n`inds`. If `ind` or any of `inds` are empty, returns true."
  [pred ind & inds])

(defn all-bindings
  {:params [(or :struct :table :nil) :boolean?] :ret @[:symbol]}
  "(all-bindings &opt env local)\n\nGet all symbols available in an environment. Defaults to the current\nfiber's environment. If `local` is truthy, will not show inherited bindings\n(from prototype tables)."
  [&opt env local])

(defn all-dynamics
  {:params [(or :struct :table :nil) :boolean?] :ret @[:keyword]}
  "(all-dynamics &opt env local)\n\nGet all dynamic bindings in an environment. Defaults to the current\nfiber's environment. If `local` is truthy, will not show inherited bindings\n(from prototype tables)."
  [&opt env local])

(defmacro and
  {:params [:any] :ret :any}
  "(and & forms)\n\nEvaluates to the last argument if all preceding elements are truthy, otherwise\nevaluates to the first falsey argument."
  [& forms])

(defn any?
  {:params [(or [a] @[a])] :ret a? :narrows :any}
  "(any? ind)\n\nEvaluates to the last element of `ind` if all preceding elements are falsey,\notherwise evaluates to the first truthy element."
  [ind])

(defn apply
  {:params [:function :any] :ret :any}
  "(apply f & args)\n\nApplies a function f to a variable number of arguments. Each element in args is used as an argument to f, except the last element in args, which is expected to be an array or a tuple. Each element in this last argument is then also pushed as an argument to f."
  [f & args])

(defn array
  {:params [:any] :ret :array}
  "(array & items)\n\nCreate a new array that contains items. Returns the new array."
  [& items])

(defn array/clear
  {:params [@[a]] :ret @[a]}
  "(array/clear arr)\n\nEmpties an array, setting it's count to 0 but does not free the backing capacity. Returns the modified array."
  [arr])

(defn array/concat
  {:params [@[a] :any] :ret @[a]}
  "(array/concat arr & parts)\n\nConcatenates a variable number of arrays (and tuples) into the first argument, which must be an array. If any of the parts are arrays or tuples, their elements will be inserted into the array. Otherwise, each part in `parts` will be appended to `arr` in order. Return the modified array `arr`."
  [arr & parts])

(defn array/ensure
  {:params [@[a] :number :number] :ret @[a]}
  "(array/ensure arr capacity growth)\n\nEnsures that the memory backing the array is large enough for `capacity` items at the given rate of growth. `capacity` and `growth` must be integers. If the backing capacity is already enough, then this function does nothing. Otherwise, the backing memory will be reallocated so that there is enough space."
  [arr capacity growth])

(defn array/fill
  {:params [@[a] a] :ret @[a]}
  "(array/fill arr &opt value)\n\nReplace all elements of an array with `value` (defaulting to nil) without changing the length of the array. Returns the modified array."
  [arr &opt value])

(defn array/insert
  {:params [@[a] :number a] :ret @[a]}
  "(array/insert arr at & xs)\n\nInsert all `xs` into array `arr` at index `at`. `at` should be an integer between 0 and the length of the array. A negative value for `at` will index backwards from the end of the array, inserting after the index such that inserting at -1 appends to the array. Returns the array."
  [arr at & xs])

(defn array/join
  {:params [@[a] (or [a] @[a])] :ret @[a]}
  "(array/join arr & parts)\n\nJoin a variable number of arrays and tuples into the first argument, which must be an array. Return the modified array `arr`."
  [arr & parts])

(defn array/new
  {:params [:number] :ret :array}
  "(array/new capacity)\n\nCreates a new empty array with a pre-allocated capacity. The same as `(array)` but can be more efficient if the maximum size of an array is known."
  [capacity])

(defn array/new-filled
  {:params [:number a] :ret @[a]}
  "(array/new-filled count &opt value)\n\nCreates a new array of `count` elements, all set to `value`, which defaults to nil. Returns the new array."
  [count &opt value])

(defn array/peek
  {:params [@[a]] :ret a?}
  "(array/peek arr)\n\nReturns the last element of the array. Does not modify the array."
  [arr])

(defn array/pop
  {:params [@[a]] :ret a?}
  "(array/pop arr)\n\nRemove the last element of the array and return it. If the array is empty, will return nil. Modifies the input array."
  [arr])

(defn array/push
  {:params [@[a] a] :ret @[a]}
  "(array/push arr & xs)\n\nPush all the elements of xs to the end of an array. Modifies the input array and returns it."
  [arr & xs])

(defn array/remove
  {:params [@[a] :number :number] :ret @[a]}
  "(array/remove arr at &opt n)\n\nRemove up to `n` elements starting at index `at` in array `arr`. `at` can index from the end of the array with a negative index, and `n` must be a non-negative integer. By default, `n` is 1. Returns the array."
  [arr at &opt n])

(defn array/slice
  {:params [(or [a] @[a]) :number? :number?] :ret @[a]}
  "(array/slice arrtup &opt start end)\n\nTakes a slice of array or tuple from `start` to `end`. The range is half open, [start, end). Indexes can also be negative, indicating indexing from the end of the array. By default, `start` is 0 and `end` is the length of the array. Note that if the range is negative, it is taken as (start, end] to allow a full negative slice range. Returns a new array."
  [arrtup &opt start end])

(defn array/trim
  {:params [@[a]] :ret @[a]}
  "(array/trim arr)\n\nSet the backing capacity of an array to its current length. Returns the modified array."
  [arr])

(defn array/weak
  {:params [:number] :ret :array}
  "(array/weak capacity)\n\nCreates a new empty array with a pre-allocated capacity and support for weak references. Similar to `array/new`."
  [capacity])

(defn array?
  {:params [:any] :ret :boolean :narrows :array}
  "(array? x)\n\nCheck if x is an array."
  [x])

(defmacro as->
  {:params [:any :symbol :any] :ret :any}
  "(as-> x as & forms)\n\nThread forms together, replacing `as` in `forms` with the value\nof the previous form. The first form is the value x. Returns the\nlast value."
  [x as & forms])

(defmacro as-macro
  {:params [:symbol :any] :ret :any}
  "(as-macro f & args)\n\nUse a function or macro literal `f` as a macro. This lets\nany function be used as a macro. Inside a quasiquote, the\nidiom `(as-macro ,my-custom-macro arg1 arg2...)` can be used\nto avoid unwanted variable capture of `my-custom-macro`."
  [f & args])

(defmacro as?->
  {:params [:any :symbol :any] :ret :any}
  "(as?-> x as & forms)\n\nThread forms together, replacing `as` in `forms` with the value\nof the previous form. The first form is the value x. If any\nintermediate values are falsey, return nil; otherwise, returns the\nlast value."
  [x as & forms])

(defn asm
  {:params [(or :struct :table)] :ret :function}
  "(asm assembly)\n\nReturns a new function that is the compiled result of the assembly.\nThe syntax for the assembly can be found on the Janet website, and should correspond\nto the return value of disasm. Will throw an\nerror on invalid assembly."
  [assembly])

(defmacro assert
  {:params [a :any] :ret a}
  "(assert x &opt err)\n\nThrow an error if x is not truthy. Will not evaluate `err` if x is truthy."
  [x &opt err])

(defmacro assertf
  {:params [a :string :any] :ret a}
  "(assertf x fmt & args)\n\nConvenience macro that combines `assert` and `string/format`."
  [x fmt & args])

(defn bad-compile
  {:params [:string :any :string :number? :number?] :ret :never}
  "(bad-compile msg macrof where &opt line col)\n\nDefault handler for a compile error."
  [msg macrof where &opt line col])

(defn bad-parse
  {:params [:abstract :string] :ret :never}
  "(bad-parse p where)\n\nDefault handler for a parse error."
  [p where])

(defn band
  {:params [:number] :ret :number}
  "(band & xs)\n\nReturns the bit-wise and of all values in xs. Each x in xs must be an integer."
  [& xs])

(defn blshift
  {:params [:number :number] :ret :number}
  "(blshift x & shifts)\n\nReturns the value of x bit shifted left by the sum of all values in shifts. x and each element in shift must be an integer."
  [x & shifts])

(defn bnot
  {:params [:number] :ret :number}
  "(bnot x)\n\nReturns the bit-wise inverse of integer x."
  [x])

(defn boolean?
  {:params [:any] :ret :boolean :narrows :boolean}
  "(boolean? x)\n\nCheck if x is a boolean."
  [x])

(defn bor
  {:params [:number] :ret :number}
  "(bor & xs)\n\nReturns the bit-wise or of all values in xs. Each x in xs must be an integer."
  [& xs])

(defn brshift
  {:params [:number :number] :ret :number}
  "(brshift x & shifts)\n\nReturns the value of x bit shifted right by the sum of all values in shifts. x and each element in shift must be an integer."
  [x & shifts])

(defn brushift
  {:params [:number :number] :ret :number}
  "(brushift x & shifts)\n\nReturns the value of x bit shifted right by the sum of all values in shifts. x and each element in shift must be an integer. The sign of x is not preserved, so for positive shifts the return value will always be positive."
  [x & shifts])

(defn buffer
  {:params [:any] :ret :buffer}
  "(buffer & xs)\n\nCreates a buffer by concatenating the elements of `xs` together. If an element is not a byte sequence, it is converted to bytes via `describe`. Returns the new buffer."
  [& xs])

(defn buffer/bit
  {:params [:buffer :number] :ret :boolean}
  "(buffer/bit buffer index)\n\nGets the bit at the given bit-index. Returns true if the bit is set, false if not."
  [buffer index])

(defn buffer/bit-clear
  {:params [:buffer :number] :ret :buffer}
  "(buffer/bit-clear buffer index)\n\nClears the bit at the given bit-index. Returns the buffer."
  [buffer index])

(defn buffer/bit-set
  {:params [:buffer :number] :ret :buffer}
  "(buffer/bit-set buffer index)\n\nSets the bit at the given bit-index. Returns the buffer."
  [buffer index])

(defn buffer/bit-toggle
  {:params [:buffer :number] :ret :buffer}
  "(buffer/bit-toggle buffer index)\n\nToggles the bit at the given bit index in buffer. Returns the buffer."
  [buffer index])

(defn buffer/blit
  {:params [:buffer (or :string :buffer :symbol :keyword) :number :number :number] :ret :buffer}
  "(buffer/blit dest src &opt dest-start src-start src-end)\n\nInsert the contents of `src` into `dest`. Can optionally take indices that indicate which part of `src` to copy into which part of `dest`. Indices can be negative in order to index from the end of `src` or `dest`. Returns `dest`."
  [dest src &opt dest-start src-start src-end])

(defn buffer/clear
  {:params [:buffer] :ret :buffer}
  "(buffer/clear buffer)\n\nSets the size of a buffer to 0 and empties it. The buffer retains its memory so it can be efficiently refilled. Returns the modified buffer."
  [buffer])

(defn buffer/fill
  {:params [:buffer :number] :ret :buffer}
  "(buffer/fill buffer &opt byte)\n\nFill up a buffer with bytes, defaulting to 0s. Does not change the buffer's length. Returns the modified buffer."
  [buffer &opt byte])

(defn buffer/format
  {:params [:buffer :string :any] :ret :buffer}
  "(buffer/format buffer format & args)\n\nSnprintf like functionality for printing values into a buffer. Returns the modified buffer."
  [buffer format & args])

(defn buffer/format-at
  {:params [:buffer :number :string :any] :ret :buffer}
  "(buffer/format-at buffer at format & args)\n\nSnprintf like functionality for printing values into a buffer. Returns the modified buffer."
  [buffer at format & args])

(defn buffer/from-bytes
  {:params [:number] :ret :buffer}
  "(buffer/from-bytes & byte-vals)\n\nCreates a buffer from integer parameters with byte values. All integers will be coerced to the range of 1 byte 0-255."
  [& byte-vals])

(defn buffer/new
  {:params [:number] :ret :buffer}
  "(buffer/new capacity)\n\nCreates a new, empty buffer with enough backing memory for `capacity` bytes. Returns a new buffer of length 0."
  [capacity])

(defn buffer/new-filled
  {:params [:number :number] :ret :buffer}
  "(buffer/new-filled count &opt byte)\n\nCreates a new buffer of length `count` filled with `byte`. By default, `byte` is 0. Returns the new buffer."
  [count &opt byte])

(defn buffer/popn
  {:params [:buffer :number] :ret :buffer}
  "(buffer/popn buffer n)\n\nRemoves the last `n` bytes from the buffer. Returns the modified buffer."
  [buffer n])

(defn buffer/push
  {:params [:buffer :any] :ret :buffer}
  "(buffer/push buffer & xs)\n\nPush both individual bytes and byte sequences to a buffer. For each x in xs, push the byte if x is an integer, otherwise push the bytesequence to the buffer. Thus, this function behaves like both `buffer/push-string` and `buffer/push-byte`. Returns the modified buffer. Will throw an error if the buffer overflows."
  [buffer & xs])

(defn buffer/push-at
  {:params [:buffer :number :any] :ret :buffer}
  "(buffer/push-at buffer index & xs)\n\nSame as buffer/push, but copies the new data into the buffer  at index `index`."
  [buffer index & xs])

(defn buffer/push-byte
  {:params [:buffer :number] :ret :buffer}
  "(buffer/push-byte buffer & xs)\n\nAppend bytes to a buffer. Will expand the buffer as necessary. Returns the modified buffer. Will throw an error if the buffer overflows."
  [buffer & xs])

(defn buffer/push-float32
  {:params [:buffer :keyword? :number] :ret :buffer}
  "(buffer/push-float32 buffer order data)\n\nPush the underlying bytes of a 32 bit float data onto the end of the buffer. Returns the modified buffer."
  [buffer order data])

(defn buffer/push-float64
  {:params [:buffer :keyword? :number] :ret :buffer}
  "(buffer/push-float64 buffer order data)\n\nPush the underlying bytes of a 64 bit float data onto the end of the buffer. Returns the modified buffer."
  [buffer order data])

(defn buffer/push-string
  {:params [:buffer (or :string :buffer :symbol :keyword)] :ret :buffer}
  "(buffer/push-string buffer & xs)\n\nPush byte sequences onto the end of a buffer. Will accept any of strings, keywords, symbols, and buffers. Returns the modified buffer. Will throw an error if the buffer overflows."
  [buffer & xs])

(defn buffer/push-uint16
  {:params [:buffer :keyword? :number] :ret :buffer}
  "(buffer/push-uint16 buffer order data)\n\nPush a 16 bit unsigned integer data onto the end of the buffer. Returns the modified buffer."
  [buffer order data])

(defn buffer/push-uint32
  {:params [:buffer :keyword? :number] :ret :buffer}
  "(buffer/push-uint32 buffer order data)\n\nPush a 32 bit unsigned integer data onto the end of the buffer. Returns the modified buffer."
  [buffer order data])

(defn buffer/push-uint64
  {:params [:buffer :keyword? :number] :ret :buffer}
  "(buffer/push-uint64 buffer order data)\n\nPush a 64 bit unsigned integer data onto the end of the buffer. Returns the modified buffer."
  [buffer order data])

(defn buffer/push-word
  {:params [:buffer :number] :ret :buffer}
  "(buffer/push-word buffer & xs)\n\nAppend machine words to a buffer. The 4 bytes of the integer are appended in twos complement, little endian order, unsigned for all x. Returns the modified buffer. Will throw an error if the buffer overflows."
  [buffer & xs])

(defn buffer/slice
  {:params [(or :string :buffer :symbol :keyword) :number? :number?] :ret :buffer}
  "(buffer/slice bytes &opt start end)\n\nTakes a slice of a byte sequence from `start` to `end`. The range is half open, [start, end). Indexes can also be negative, indicating indexing from the end of the end of the array. By default, `start` is 0 and `end` is the length of the buffer. Returns a new buffer."
  [bytes &opt start end])

(defn buffer/trim
  {:params [:buffer] :ret :buffer}
  "(buffer/trim buffer)\n\nSet the backing capacity of the buffer to the current length of the buffer. Returns the modified buffer."
  [buffer])

(defn buffer?
  {:params [:any] :ret :boolean :narrows :buffer}
  "(buffer? x)\n\nCheck if x is a buffer."
  [x])

(defn bundle/add
  {:params [:table :string :string? :any] :ret :string}
  "(bundle/add manifest src &opt dest chmod-mode)\n\nAdd a file or directory during an install relative to `(dyn *syspath*)`.\nAdded files and directories will be recorded in the bundle manifest such\nthat they are properly tracked and removed during an upgrade or uninstall."
  [manifest src &opt dest chmod-mode])

(defn bundle/add-bin
  {:params [:table :string :string? :any] :ret :string}
  "(bundle/add-bin manifest src &opt filename chmod-mode)\n\nAdd a file to the \"bin\" subdirectory of the current syspath. By default,\nfiles will be set to be executable."
  [manifest src &opt filename chmod-mode])

(defn bundle/add-directory
  {:params [:table :string :any] :ret :string}
  "(bundle/add-directory manifest dest &opt chmod-mode)\n\nAdd a directory during an install relative to `(dyn *syspath*)`."
  [manifest dest &opt chmod-mode])

(defn bundle/add-file
  {:params [:table :string :string? :any] :ret :string}
  "(bundle/add-file manifest src &opt dest chmod-mode)\n\nAdd a file during an install relative to `(dyn *syspath*)`."
  [manifest src &opt dest chmod-mode])

(defn bundle/add-manpage
  {:params [:table :string :string?] :ret :string}
  "(bundle/add-manpage manifest src &opt mansec)\n\nAdd a file to the man subdirectory of the current syspath. Files are\ncopied inside a directory `mansec`. By default, `mansec` is \"man1\"."
  [manifest src &opt mansec])

(defn bundle/install
  {:params [:string :any] :ret :string}
  "(bundle/install path &keys config)\n\nInstall a bundle from the local filesystem. The name of the bundle is\nthe value mapped to :name in either `config` or the info file. There are\n5 hooks called during installation (postdeps, clean, build, install and\ncheck). A user can register a hook by defining a function with the same name\nin the bundle script."
  [path &keys config])

(defn bundle/installed?
  {:params [:string] :ret :boolean :narrows :any}
  "(bundle/installed? bundle-name)\n\nCheck if a bundle is installed."
  [bundle-name])

(defn bundle/list
  {:params [] :ret @[:string]}
  "(bundle/list)\n\nGet a list of all installed bundles in lexical order."
  [])

(defn bundle/manifest
  {:params [:string] :ret (or :struct :table)}
  "(bundle/manifest bundle-name)\n\nGet the manifest for a given installed bundle."
  [bundle-name])

(defn bundle/prune
  {:params [] :ret :nil}
  "(bundle/prune)\n\nRemove all orphaned bundles from the current syspath. An orphaned bundle is a\nbundle that is marked for :auto-remove and is not depended on by any other bundle."
  [])

(defn bundle/reinstall
  {:params [:string :any] :ret :string}
  "(bundle/reinstall bundle-name &keys new-config)\n\nReinstall an existing bundle from the local source code."
  [bundle-name &keys new-config])

(defn bundle/replace
  {:params [:string :string :any] :ret :string}
  "(bundle/replace bundle-name path &keys new-config)\n\nReinstall an existing bundle from a new directory. Similar to\nbundle/reinstall, but installs the replacement bundle from any directory.\nThis is necessary to replace a package without breaking any dependencies."
  [bundle-name path &keys new-config])

(defn bundle/topolist
  {:params [] :ret @[:string]}
  "(bundle/topolist)\n\nGet topological order of all bundles, such that each bundle is listed after its dependencies."
  [])

(defn bundle/uninstall
  {:params [:string] :ret :nil}
  "(bundle/uninstall bundle-name)\n\nRemove a bundle from the current syspath. There is 1 hook called during\nuninstallation (uninstall). A user can register a hook by defining a\nfunction with the same name in the bundle script."
  [bundle-name])

(defn bundle/update-all
  {:params [:any] :ret :nil}
  "(bundle/update-all &keys configs)\n\nReinstall all bundles."
  [&keys configs])

(defn bundle/whois
  {:params [:string] :ret :string?}
  "(bundle/whois path)\n\nGiven a file path, figure out which bundle installed it."
  [path])

(defn bxor
  {:params [:number] :ret :number}
  "(bxor & xs)\n\nReturns the bit-wise xor of all values in xs. Each x in xs must be an integer."
  [& xs])

(defn bytes?
  {:params [:any] :ret :boolean :narrows (or :string :buffer :symbol :keyword)}
  "(bytes? x)\n\nCheck if x is a string, symbol, keyword, or buffer."
  [x])

(defn cancel
  {:params [:fiber :any] :ret :any}
  "(cancel fiber err)\n\nResume a fiber but have it immediately raise an error. This lets a programmer unwind a pending fiber. Returns the same result as resume."
  [fiber err])

(defmacro case
  {:params [:any :any] :ret :any}
  "(case dispatch & pairs)\n\nSelect the body that equals the dispatch value. When `pairs`\nhas an odd number of elements, the last is the default expression.\nIf no match is found, returns nil."
  [dispatch & pairs])

(defmacro catseq
  {:params [:tuple :any] :ret :array}
  "(catseq head & body)\n\nSimilar to `loop`, but concatenates each element from the loop body into an array and returns that.\nSee `loop` for details."
  [head & body])

(defn cfunction?
  {:params [:any] :ret :boolean :narrows :cfunction}
  "(cfunction? x)\n\nCheck if x is a cfunction."
  [x])

(defmacro chr
  {:params [:string] :ret :number}
  "(chr c)\n\nConvert a string of length 1 to its byte (ascii) value at compile time."
  [c])

(defn cli-main
  {:params [(or [:string] @[:string])] :ret :any}
  "(cli-main args)\n\nEntrance for the Janet CLI tool. Call this function with the command line\narguments as an array or tuple of strings to invoke the CLI interface."
  [args])

(defn cmp
  {:params [:any :any] :ret :number}
  "(cmp x y)\n\nReturns -1 if x is strictly less than y, 1 if y is strictly greater than x, and 0 otherwise. To return 0, x and y must be the exact same type."
  [x y])

(defmacro comment
  {:params [] :ret :nil}
  "(comment &)\n\nIgnores the body of the comment."
  [&])

(defn comp
  {:params [:function] :ret :function}
  "(comp & functions)\n\nTakes multiple functions and returns a function that is the composition\nof those functions."
  [& functions])

(defn compare
  {:params [:any :any] :ret :number}
  "(compare x y)\n\nPolymorphic compare. Returns -1, 0, 1 for x < y, x = y, x > y respectively.\nDiffers from the primitive comparators in that it first checks to\nsee whether either x or y implement a `compare` method which can\ncompare x and y. If so, it uses that method. If not, it\ndelegates to the primitive comparators."
  [x y])

(defn compare<
  {:params [:any] :ret :boolean}
  "(compare< & xs)\n\nEquivalent of `<` but using polymorphic `compare` instead of primitive comparator."
  [& xs])

(defn compare<=
  {:params [:any] :ret :boolean}
  "(compare<= & xs)\n\nEquivalent of `<=` but using polymorphic `compare` instead of primitive comparator."
  [& xs])

(defn compare=
  {:params [:any] :ret :boolean}
  "(compare= & xs)\n\nEquivalent of `=` but using polymorphic `compare` instead of primitive comparator."
  [& xs])

(defn compare>
  {:params [:any] :ret :boolean}
  "(compare> & xs)\n\nEquivalent of `>` but using polymorphic `compare` instead of primitive comparator."
  [& xs])

(defn compare>=
  {:params [:any] :ret :boolean}
  "(compare>= & xs)\n\nEquivalent of `>=` but using polymorphic `compare` instead of primitive comparator."
  [& xs])

(defmacro compif
  {:params [:any :any :any] :ret :any}
  "(compif cnd tru &opt fals)\n\nCheck the condition `cnd` at compile time -- if truthy, compile `tru`, else compile `fals`."
  [cnd tru &opt fals])

(defn compile
  {:params [:any :table :any :array] :ret (or :function :struct)}
  "(compile ast &opt env source lints)\n\nCompiles an Abstract Syntax Tree (ast) into a function. Pair the compile function with parsing functionality to implement eval. Returns a new function and does not modify ast. Returns an error struct with keys :line, :column, and :error if compilation fails. If a `lints` array is given, linting messages will be appended to the array. Each message will be a tuple of the form `(level line col message)`."
  [ast &opt env source lints])

(defn complement
  {:params [:function] :ret :function}
  "(complement f)\n\nReturns a function that is the complement to the argument."
  [f])

(defmacro comptime
  {:params [:any] :ret :any}
  "(comptime x)\n\nEvals x at compile time and returns the result. Similar to a top level unquote."
  [x])

(defmacro compwhen
  {:params [:any :any] :ret :any}
  "(compwhen cnd & body)\n\nCheck the condition `cnd` at compile time -- if truthy, compile `(upscope ;body)`, else compile nil."
  [cnd & body])

(defmacro cond
  {:params [:any] :ret :any}
  "(cond & pairs)\n\nEvaluates conditions sequentially until the first true condition\nis found, and then executes the corresponding body. If there are an\nodd number of forms, and no forms are matched, the last expression\nis executed. If there are no matches, returns nil."
  [& pairs])

(defmacro coro
  {:params [:any] :ret :fiber}
  "(coro & body)\n\nA wrapper for making fibers that may yield multiple values (coroutine). Same as `(fiber/new (fn [] ;body) :yi)`."
  [& body])

(defn count
  {:params [(fn [a] :any) (or [a] @[a]) (or [:any] @[:any])] :ret :number}
  "(count pred ind & inds)\n\nCount the number of values in a data structure `ind` for which\napplying `pred` yields a truthy value, but only if no `inds` are\nprovided. Multiple data structures can be handled if each `inds`\nis a data structure and `pred` is a function of arity one more\nthan the number of `inds`. Note that `pred` is only applied to\nvalues at indeces up to the largest index of the shortest of\n`ind` and each of `inds`."
  [pred ind & inds])

(defn curenv
  {:params [:number?] :ret :table}
  "(curenv &opt n)\n\nGet the current environment table. Same as `(fiber/getenv (fiber/current))`. If `n`\nis provided, gets the nth prototype of the environment table."
  [&opt n])

(defn debug
  {:params [:any] :ret :nil}
  "(debug &opt x)\n\nThrows a debug signal that can be caught by a parent fiber and used to inspect the running state of the current fiber. Returns the value passed in by resume."
  [&opt x])

(defn debug/arg-stack
  {:params [:fiber] :ret :array}
  "(debug/arg-stack fiber)\n\nGets all values currently on the fiber's argument stack. Normally, this should be empty unless the fiber signals while pushing arguments to make a function call. Returns a new array."
  [fiber])

(defn debug/break
  {:params [:string :number :number] :ret :nil}
  "(debug/break source line col)\n\nSets a breakpoint in `source` at a given line and column. Will throw an error if the breakpoint location cannot be found. For example\n\n\t(debug/break \"core.janet\" 10 4)\n\nwill set a breakpoint at line 10, 4th column of the file core.janet."
  [source line col])

(defn debug/fbreak
  {:params [:function :number?] :ret :nil}
  "(debug/fbreak fun &opt pc)\n\nSet a breakpoint in a given function. pc is an optional offset, which is in bytecode instructions. fun is a function value. Will throw an error if the offset is too large or negative."
  [fun &opt pc])

(defn debug/lineage
  {:params [:fiber] :ret :array}
  "(debug/lineage fib)\n\nReturns an array of all child fibers from a root fiber. This function is useful when a fiber signals or errors to an ancestor fiber. Using this function, the fiber handling the error can see which fiber raised the signal. This function should be used mostly for debugging purposes."
  [fib])

(defn debug/stack
  {:params [:fiber] :ret :array}
  "(debug/stack fib)\n\nGets information about the stack as an array of tables. Each table in the array contains information about a stack frame. The top-most, current stack frame is the first table in the array, and the bottom-most stack frame is the last value. Each stack frame contains some of the following attributes:\n\n* :c - true if the stack frame is a c function invocation\n\n* :source-column - the current source column of the stack frame\n\n* :function - the function that the stack frame represents\n\n* :source-line - the current source line of the stack frame\n\n* :name - the human-friendly name of the function\n\n* :pc - integer indicating the location of the program counter\n\n* :source - string with the file path or other identifier for the source code\n\n* :slots - array of all values in each slot\n\n* :tail - boolean indicating a tail call"
  [fib])

(defn debug/stacktrace
  {:params [:fiber :any :string?] :ret :fiber}
  "(debug/stacktrace fiber &opt err prefix)\n\nPrints a nice looking stacktrace for a fiber. Can optionally provide an error value to print the stack trace with. If `prefix` is nil or not provided, will skip the error line. Returns the fiber."
  [fiber &opt err prefix])

(defn debug/step
  {:params [:fiber :any] :ret :any} # TODO: the C source does not say what it returns
  "(debug/step fiber &opt x)\n\nRun a fiber for one virtual instruction of the Janet machine. Can optionally pass in a value that will be passed as the resuming value. Returns the signal value, which will usually be nil, as breakpoints raise nil signals."
  [fiber &opt x])

(defn debug/unbreak
  {:params [:string :number :number] :ret :nil}
  "(debug/unbreak source line column)\n\nRemove a breakpoint with a source key at a given line and column. Will throw an error if the breakpoint cannot be found."
  [source line column])

(defn debug/unfbreak
  {:params [:function :number?] :ret :nil}
  "(debug/unfbreak fun &opt pc)\n\nUnset a breakpoint set with debug/fbreak."
  [fun &opt pc])

(defn debugger
  {:params [:fiber :number?] :ret :any}
  "(debugger fiber &opt level)\n\nRun a repl-based debugger on a fiber. Optionally pass in a level  to differentiate nested debuggers."
  [fiber &opt level])

(def debugger-env
  {:type :table}
  "An environment that contains dot prefixed functions for debugging."
  nil)

(defn debugger-on-status
  {:params [:table :number? :boolean?] :ret :function}
  "(debugger-on-status env &opt level is-repl)\n\nCreate a function that can be passed to `run-context`'s `:on-status`  argument that will drop into a debugger on errors. The debugger will  only start on abnormal signals if the env table has the `:debug` dyn  set to a truthy value."
  [env &opt level is-repl])

(defn dec
  {:params [:number] :ret :number}
  "(dec x)\n\nReturns x - 1."
  [x])

(defn deep-not=
  {:params [:any :any] :ret :boolean}
  "(deep-not= x y)\n\nLike `not=`, but mutable types (arrays, tables, buffers) are considered\nequal if they have identical structure. Much slower than `not=`."
  [x y])

(defn deep=
  {:params [:any :any] :ret :boolean}
  "(deep= x y)\n\nLike `=`, but mutable types (arrays, tables, buffers) are considered\nequal if they have identical structure. Much slower than `=`."
  [x y])

(defmacro def-
  {:params [:symbol :any] :ret :any}
  "(def- name & more)\n\nDefine a private value that will not be exported."
  [name & more])

(defmacro default
  {:params [:symbol a] :ret a}
  "(default sym val)\n\nDefine a default value for an optional argument.\nExpands to `(def sym (if (= nil sym) val sym))`."
  [sym val])

(def default-peg-grammar
  {:type :table}
  "The default grammar used for pegs. This grammar defines several common patterns\nthat should make it easier to write more complex patterns."
  nil)

(defmacro defdyn
  {:params [:symbol :any] :ret :keyword}
  "(defdyn alias & more)\n\nDefine an alias for a keyword that is used as a dynamic binding. The\nalias is a normal, lexically scoped binding that can be used instead of\na keyword to prevent typos. `defdyn` does not set dynamic bindings or otherwise\nreplace `dyn` and `setdyn`. The alias *must* start and end with the `*` character, usually\ncalled \"earmuffs\"."
  [alias & more])

(defmacro defer
  {:params [:any :any] :ret :any}
  "(defer form & body)\n\nRun `form` unconditionally after `body`, even if the body throws an error.\nWill also run `form` if a user signal 0-4 is received."
  [form & body])

(defn defglobal
  {:params [:symbol :any] :ret :nil}
  "(defglobal name value)\n\nDynamically create a global def."
  [name value])

(defmacro defmacro
  {:params [:symbol :any] :ret :function}
  "(defmacro name & more)\n\nDefine a macro."
  [name & more])

(defmacro defmacro-
  {:params [:symbol :any] :ret :function}
  "(defmacro- name & more)\n\nDefine a private macro that will not be exported."
  [name & more])

(defmacro defn
  {:params [:symbol :any] :ret :function}
  "(defn name & more)\n\nDefine a function. Equivalent to `(def name (fn name [args] ...))`."
  [name & more])

(defmacro defn-
  {:params [:symbol :any] :ret :function}
  "(defn- name & more)\n\nDefine a private function that will not be exported."
  [name & more])

(defmacro delay
  {:params [:any] :ret :function}
  "(delay & forms)\n\nLazily evaluate a series of expressions. Returns a function that  returns the result of the last expression. Will only evaluate the  body once, and then memoizes the result."
  [& forms])

(defn describe
  {:params [:any] :ret :string}
  "(describe x)\n\nReturns a string that is a human-readable description of `x`. For recursive data structures, the string returned contains a pointer value from which the identity of `x` can be determined."
  [x])

(defn dictionary?
  {:params [:any] :ret :boolean :narrows (or :struct :table)}
  "(dictionary? x)\n\nCheck if x is a table or struct."
  [x])

(defn disasm
  {:params [:function :keyword?] :ret :any} # TODO: the C source does not say what it returns
  "(disasm func &opt field)\n\nReturns assembly that could be used to compile the given function. func must be a function, not a c function. Will throw on error on a badly typed argument. If given a field name, will only return that part of the function assembly. Possible fields are:\n\n* :arity - number of required and optional arguments.\n* :min-arity - minimum number of arguments function can be called with.\n* :max-arity - maximum number of arguments function can be called with.\n* :vararg - true if function can take a variable number of arguments.\n* :structarg - true if function can take a variable number of arguments using the &keys option.\n* :namedargs - if function can take a variable number of arguments using the &named option, this will be the number of named arguments.\n* :bytecode - array of parsed bytecode instructions. Each instruction is a tuple.\n* :source - name of source file that this function was compiled from.\n* :name - name of function.\n* :slotcount - how many virtual registers, or slots, this function uses. Corresponds to stack space used by function.\n* :symbolmap - all symbols and their slots.\n* :constants - an array of constants referenced by this function.\n* :sourcemap - a mapping of each bytecode instruction to a line and column in the source file.\n* :environments - an internal mapping of which enclosing functions are referenced for bindings.\n* :defs - other function definitions that this function may instantiate.\n"
  [func &opt field])

(defn distinct
  {:params [(or [a] @[a])] :ret @[a]}
  "(distinct xs)\n\nReturns an array of the deduplicated values in `xs`."
  [xs])

(defn div
  {:params [:number] :ret :number}
  "(div & xs)\n\nReturns the floored division of xs. If xs is empty, returns 1. If xs has one value x, returns the reciprocal of x. Otherwise return the first value of xs repeatedly divided by the remaining values."
  [& xs])

(defmacro doc
  {:params [:any] :ret :nil}
  "(doc &opt sym)\n\nShows documentation for the given symbol, or can show a list of available bindings.\nIf `sym` is a symbol, will look for documentation for that symbol. If `sym` is a string\nor is not provided, will show all lexical and dynamic bindings in the current environment\ncontaining that string (all bindings will be shown if no string is given)."
  [&opt sym])

(defn doc*
  {:params [:any] :ret :nil}
  "(doc* &opt sym)\n\nGet the documentation for a symbol in a given environment. Function form of `doc`."
  [&opt sym])

(defn doc-format
  {:params [:string :number? :number? :boolean?] :ret :buffer}
  "(doc-format str &opt width indent colorize)\n\nReformat a docstring to wrap a certain width. Docstrings can either be plaintext\nor a subset of markdown. This allows a long single line of prose or formatted text to be\na well-formed docstring. Returns a buffer containing the formatted text."
  [str &opt width indent colorize])

(defn doc-of
  {:params [:any] :ret :string}
  "(doc-of x)\n\nSearches all loaded modules in module/cache for a given binding and prints out its documentation.\nThis does a search by value instead of by name. Returns nil."
  [x])

(defn dofile
  {:params [:any :any :any :any :any :any :any :any] :ret :any}
  "(dofile path &named exit env source expander evaluator read parser)\n\nEvaluate a file, file path, or stream and return the resulting environment. :env, :expander,\n:source, :evaluator, :read, and :parser are passed through to the underlying\n`run-context` call. If `exit` is true, any top level errors will trigger a\ncall to `(os/exit 1)` after printing the error."
  [path &named exit env source expander evaluator read parser])

(defn drop
  {:params [:number (or [a] @[a] :string :buffer)] :ret (or [a] :string)}
  "(drop n ind)\n\nDrop the first `n` elements in an indexed or bytes type. Returns a new tuple or string\ninstance, respectively. If `n` is negative, drops the last `n` elements instead."
  [n ind])

(defn drop-until
  {:params [(fn [a] :any) (or [a] @[a] :string :buffer)] :ret (or [a] :string)}
  "(drop-until pred ind)\n\nSame as `(drop-while (complement pred) ind)`."
  [pred ind])

(defn drop-while
  {:params [(fn [a] :any) (or [a] @[a] :string :buffer)] :ret (or [a] :string)}
  "(drop-while pred ind)\n\nGiven a predicate, remove elements from an indexed or bytes type that satisfy\nthe predicate, and abort on first failure. Returns a new tuple or string, respectively."
  [pred ind])

(defn dyn
  {:params [:keyword :any] :ret :any} # TODO: the C source does not say what it returns
  "(dyn key &opt default)\n\nGet a dynamic binding. Returns the default value (or nil) if no binding found."
  [key &opt default])

(defmacro each
  {:params [:any :any :any] :ret :nil}
  "(each x ds & body)\n\nLoop over each value in `ds`. Returns nil."
  [x ds & body])

(defmacro eachk
  {:params [:any :any :any] :ret :nil}
  "(eachk x ds & body)\n\nLoop over each key in `ds`. Returns nil."
  [x ds & body])

(defmacro eachp
  {:params [:any :any :any] :ret :nil}
  "(eachp x ds & body)\n\nLoop over each (key, value) pair in `ds`. Returns nil."
  [x ds & body])

(defmacro edefer
  {:params [:any :any] :ret :any}
  "(edefer form & body)\n\nRun `form` after `body` in the case that body terminates abnormally (an error or user signal 0-4).\nOtherwise, return last form in `body`."
  [form & body])

(defn eflush
  {:params [] :ret :nil}
  "(eflush)\n\nFlush `(dyn :err stderr)` if it is a file, otherwise do nothing."
  [])

(defn empty?
  {:params [:any] :ret :boolean :narrows :any}
  "(empty? iter)\n\nCheck if an iterable, `iter`, is empty."
  [iter])

(defn env-lookup
  {:params [:table] :ret :table}
  "(env-lookup env)\n\nCreates a forward lookup table for unmarshalling from an environment. To create a reverse lookup table, use the invert function to swap keys and values in the returned table."
  [env])

(defn eprin
  {:params [:any] :ret :nil}
  "(eprin & xs)\n\nSame as `prin`, but uses `(dyn :err stderr)` instead of `(dyn :out stdout)`."
  [& xs])

(defn eprinf
  {:params [:string :any] :ret :nil}
  "(eprinf fmt & xs)\n\nLike `eprintf` but with no trailing newline."
  [fmt & xs])

(defn eprint
  {:params [:any] :ret :nil}
  "(eprint & xs)\n\nSame as `print`, but uses `(dyn :err stderr)` instead of `(dyn :out stdout)`."
  [& xs])

(defn eprintf
  {:params [:string :any] :ret :nil}
  "(eprintf fmt & xs)\n\nPrints output formatted as if with `(string/format fmt ;xs)` to `(dyn :err stderr)` with a trailing newline."
  [fmt & xs])

(defn error
  {:params [:any] :ret :never}
  "(error e)\n\nThrows an error e that can be caught and handled by a parent fiber."
  [e])

(defn errorf
  {:params [:string :any] :ret :never}
  "(errorf fmt & args)\n\nA combination of `error` and `string/format`. Equivalent to `(error (string/format fmt ;args))`."
  [fmt & args])

(defn ev/acquire-lock
  {:params [:abstract] :ret :abstract}
  "(ev/acquire-lock lock)\n\nAcquire a lock such that this operating system thread is the only thread with access to this resource. This will block this entire thread until the lock becomes available, and will not yield to other fibers on this system thread."
  [lock])

(defn ev/acquire-rlock
  {:params [:abstract] :ret :abstract}
  "(ev/acquire-rlock rwlock)\n\nAcquire a read lock an a read-write lock."
  [rwlock])

(defn ev/acquire-wlock
  {:params [:abstract] :ret :abstract}
  "(ev/acquire-wlock rwlock)\n\nAcquire a write lock on a read-write lock."
  [rwlock])

(defn ev/all-tasks
  {:params [] :ret :array}
  "(ev/all-tasks)\n\nGet an array of all active task fibers that are being used by the scheduler."
  [])

(defn ev/call
  {:params [:function :any] :ret :fiber}
  "(ev/call f & args)\n\nCall a function asynchronously.\nReturns a task fiber that is scheduled to run the function."
  [f & args])

(defn ev/cancel
  {:params [:fiber :any] :ret :fiber}
  "(ev/cancel fiber err)\n\nCancel a suspended task fiber in the event loop. Differs from `cancel` in that it returns the canceled fiber immediately."
  [fiber err])

(defn ev/capacity
  {:params [:abstract] :ret :number}
  "(ev/capacity channel)\n\nGet the number of items a channel will store before blocking writers."
  [channel])

(defn ev/chan
  {:params [:number?] :ret :abstract}
  "(ev/chan &opt capacity)\n\nCreate a new channel. capacity is the number of values to queue before blocking writers, defaults to 0 if not provided. Returns a new channel."
  [&opt capacity])

(defn ev/chan-close
  {:params [:abstract] :ret :abstract}
  "(ev/chan-close chan)\n\nClose a channel. A closed channel will cause all pending reads and writes to return nil. Returns the channel."
  [chan])

(defn ev/chunk
  {:params [:abstract :number :buffer? :number?] :ret (or :buffer :nil)}
  "(ev/chunk stream n &opt buffer timeout)\n\nSame as ev/read, but will not return early if less than n bytes are available. If an end of stream is reached, will also return early with the collected bytes."
  [stream n &opt buffer timeout])

(defn ev/close
  {:params [:abstract] :ret :abstract}
  "(ev/close stream)\n\nClose a stream. This should be the same as calling (:close stream) for all streams."
  [stream])

(defn ev/count
  {:params [:abstract] :ret :number}
  "(ev/count channel)\n\nGet the number of items currently waiting in a channel."
  [channel])

(defn ev/deadline
  {:params [:number :fiber? :fiber? :boolean?] :ret :fiber}
  "(ev/deadline sec &opt tocancel tocheck intr?)\n\nSchedules the event loop to try to cancel the `tocancel` task as with `ev/cancel`. After `sec` seconds, the event loop will attempt cancellation of `tocancel` if the `tocheck` fiber is resumable. `sec` is a number that can have a fractional part. `tocancel` defaults to `(fiber/root)`, but if specified, must be a task (root fiber). `tocheck` defaults to `(fiber/current)`, but if specified, must be a fiber. Returns `tocancel` immediately. If `interrupt?` is set to true, will create a background thread to try to interrupt the VM if the timeout expires."
  [sec &opt tocancel tocheck intr?])

(defmacro ev/do-thread
  {:params [:any] :ret :any}
  "(ev/do-thread & body)\n\nRun some code in a new thread. Suspends the current fiber until the thread is complete, and\nevaluates to nil."
  [& body])

(defn ev/full
  {:params [:abstract] :ret :boolean}
  "(ev/full channel)\n\nCheck if a channel is full or not."
  [channel])

(defmacro ev/gather
  {:params [:any] :ret :array}
  "(ev/gather & bodies)\n\nCreate and run a number of fibers in parallel (created from `bodies`) and resume the\ncurrent fiber after they complete. Shorthand for `ev/go-gather`. Returns the gathered results in an\narray."
  [& bodies])

(defn ev/give
  {:params [:abstract :any] :ret :abstract}
  "(ev/give channel value)\n\nWrite a value to a channel, suspending the current fiber if the channel is full. Returns the channel if the write succeeded, nil otherwise."
  [channel value])

(defn ev/give-supervisor
  {:params [:keyword :any] :ret :nil}
  "(ev/give-supervisor tag & payload)\n\nSend a message to the current supervisor channel if there is one. The message will be a tuple of all of the arguments combined into a single message, where the first element is tag. By convention, tag should be a keyword indicating the type of message. Returns nil."
  [tag & payload])

(defn ev/go
  {:params [(or :fiber :function) :any :abstract?] :ret :fiber}
  "(ev/go fiber-or-fun &opt value supervisor)\n\nPut a fiber on the event loop to be resumed later. If a function is used, it is wrapped with `fiber/new` first. Returns a task fiber. Optionally pass a value to resume with, otherwise resumes with nil. An optional `core/channel` can be provided as a supervisor. When various events occur in the newly scheduled fiber, an event will be pushed to the supervisor. If not provided, the new fiber will inherit the current supervisor."
  [fiber-or-fun &opt value supervisor])

(defn ev/go-gather
  {:params [:function] :ret @[:any]}
  "(ev/go-gather thunks)\n\nRun a dyanmic number of fibers in parallel and resume the current fiber after they complete. Takes\nan array of functions or fibers, `thunks`, that will be run via `ev/go` in another task.\nReturns the gathered results in an array."
  [thunks])

(defn ev/lock
  {:params [] :ret :abstract}
  "(ev/lock)\n\nCreate a new lock to coordinate threads."
  [])

(defn ev/read
  {:params [:abstract :number :buffer? :number?] :ret (or :buffer :nil)}
  "(ev/read stream n &opt buffer timeout)\n\nRead up to n bytes into a buffer asynchronously from a stream. `n` can also be the keyword `:all` to read into the buffer until end of stream. Optionally provide a buffer to write into as well as a timeout in seconds after which to cancel the operation and raise an error. Returns the buffer if the read was successful or nil if end-of-stream reached. Will raise an error if there are problems with the IO operation."
  [stream n &opt buffer timeout])

(defn ev/release-lock
  {:params [:abstract] :ret :abstract}
  "(ev/release-lock lock)\n\nRelease a lock such that other threads may acquire it."
  [lock])

(defn ev/release-rlock
  {:params [:abstract] :ret :abstract}
  "(ev/release-rlock rwlock)\n\nRelease a read lock on a read-write lock"
  [rwlock])

(defn ev/release-wlock
  {:params [:abstract] :ret :abstract}
  "(ev/release-wlock rwlock)\n\nRelease a write lock on a read-write lock"
  [rwlock])

(defn ev/rselect
  {:params [:any] :ret [:keyword :any]}
  "(ev/rselect & clauses)\n\nSimilar to ev/select, but will try clauses in a random order for fairness."
  [& clauses])

(defn ev/rwlock
  {:params [] :ret :abstract}
  "(ev/rwlock)\n\nCreate a new read-write lock to coordinate threads."
  [])

(defn ev/select
  {:params [:any] :ret [:keyword :any]}
  "(ev/select & clauses)\n\nBlock until the first of several channel operations occur. Returns a tuple of the form [:give chan], [:take chan x], or [:close chan], where a :give tuple is the result of a write and a :take tuple is the result of a read. Each clause must be either a channel (for a channel take operation) or a tuple [channel x] (for a channel give operation). Operations are tried in order such that earlier clauses take precedence over later clauses. Both give and take operations can return a [:close chan] tuple, which indicates that the specified channel was closed while waiting, or that the channel was already closed."
  [& clauses])

(defn ev/sleep
  {:params [:number] :ret :nil}
  "(ev/sleep sec)\n\nSuspend the current fiber for sec seconds without blocking the event loop."
  [sec])

(defmacro ev/spawn
  {:params [:any] :ret :fiber}
  "(ev/spawn & body)\n\nRun some code in a new task fiber. This is shorthand for\n`(ev/go (fn [] ;body))`.\""
  [& body])

(defmacro ev/spawn-thread
  {:params [:any] :ret :fiber}
  "(ev/spawn-thread & body)\n\nRun some code in a new thread. Like `ev/do-thread`, but returns nil immediately."
  [& body])

(defn ev/take
  {:params [:abstract] :ret :any} # TODO: the C source does not say what it returns
  "(ev/take channel)\n\nRead from a channel, suspending the current fiber if no value is available."
  [channel])

(defn ev/thread
  {:params [(or :fiber :function) :any :keyword :abstract?] :ret :nil}
  "(ev/thread main &opt value flags supervisor)\n\nRun `main` in a new operating system thread, optionally passing `value` to resume with. The parameter `main` can either be a fiber, or a function that accepts 0 or 1 arguments. Unlike `ev/go`, this function will suspend the current fiber until the thread is complete. If you want to run the thread without waiting for a result, pass the `:n` flag to return nil immediately. Otherwise, returns nil. Available flags:\n\n* `:n` - return immediately\n* `:t` - set the task-id of the new thread to value. The task-id is passed in messages to the supervisor channel.\n* `:a` - don't copy abstract registry to new thread (performance optimization)\n* `:c` - don't copy cfunction registry to new thread (performance optimization)"
  [main &opt value flags supervisor])

(defn ev/thread-chan
  {:params [:number?] :ret :abstract}
  "(ev/thread-chan &opt limit)\n\nCreate a threaded channel. A threaded channel is a channel that can be shared between threads and used to communicate between any number of operating system threads."
  [&opt limit])

(defn ev/to-file
  {:params [] :ret :abstract}
  "(ev/to-file)\n\nCreate core/file copy of the stream. This value can be used when blocking IO behavior is needed."
  [])

(defmacro ev/with-deadline
  {:params [:number :any] :ret :any}
  "(ev/with-deadline sec & body)\n\nCreate a fiber to execute `body`, schedule the event loop to cancel\nthe task (root fiber) associated with `body`'s fiber, and start\n`body`'s fiber by resuming it.\n\nThe event loop will try to cancel the root fiber if `body`'s fiber\nhas not completed after at least `sec` seconds.\n\n`sec` is a number that can have a fractional part."
  [sec & body])

(defmacro ev/with-lock
  {:params [:any :any] :ret :any}
  "(ev/with-lock lock & body)\n\nRun a body of code after acquiring a lock. Will automatically release the lock when done."
  [lock & body])

(defmacro ev/with-rlock
  {:params [:any :any] :ret :any}
  "(ev/with-rlock lock & body)\n\nRun a body of code after acquiring read access to an rwlock. Will automatically release the lock when done."
  [lock & body])

(defmacro ev/with-wlock
  {:params [:any :any] :ret :any}
  "(ev/with-wlock lock & body)\n\nRun a body of code after acquiring write access to an rwlock. Will automatically release the lock when done."
  [lock & body])

(defn ev/write
  {:params [:abstract (or :string :buffer) :number?] :ret :nil}
  "(ev/write stream data &opt timeout)\n\nWrite data to a stream, suspending the current fiber until the write completes. Takes an optional timeout in seconds, after which will return nil. Returns nil, or raises an error if the write failed."
  [stream data &opt timeout])

(defn eval
  {:params [:any (or :struct :table :nil)] :ret :any}
  "(eval form &opt env)\n\nEvaluates a form in the current environment. If more control over the\nenvironment is needed, use `run-context`. Optionally pass in an `env` table with available bindings."
  [form &opt env])

(defn eval-string
  {:params [(or :string :buffer) (or :struct :table :nil)] :ret :any}
  "(eval-string str &opt env)\n\nEvaluates a string in the current environment. If more control over the\nenvironment is needed, use `run-context`. Optionally pass in an `env` table with available bindings."
  [str &opt env])

(defn even?
  {:params [:number] :ret :boolean :narrows :any}
  "(even? x)\n\nCheck if x is even."
  [x])

(defn every?
  {:params [(or [a] @[a])] :ret (or a :boolean) :narrows :any}
  "(every? ind)\n\nEvaluates to the last element of `ind` if all preceding elements are truthy,\notherwise evaluates to the first falsey element."
  [ind])

(defn extreme
  {:params [(fn [a a] :any) (or [a] @[a])] :ret a?}
  "(extreme order args)\n\nReturns the most extreme value in `args` based on the function `order`.\n`order` should take two values and return true or false (a comparison).\nReturns nil if `args` is empty."
  [order args])

(defn false?
  {:params [:any] :ret :boolean :narrows :any}
  "(false? x)\n\nCheck if x is false."
  [x])

(defn ffi/align
  {:params [CType] :ret :number}
  "(ffi/align type)\n\nGet the align of an ffi type in bytes."
  [type])

(defn ffi/call
  {:params [:pointer :abstract :any] :ret :any} # TODO: the C source does not say what it returns
  "(ffi/call pointer signature & args)\n\nCall a raw pointer as a function pointer. The function signature specifies how Janet values in `args` are converted to native machine types."
  [pointer signature & args])

(defn ffi/calling-conventions
  {:params [] :ret :array}
  "(ffi/calling-conventions)\n\nGet an array of all supported calling conventions on the current architecture. Some architectures may have some FFI functionality (ffi/malloc, ffi/free, ffi/read, ffi/write, etc.) but not support any calling conventions. This function can be used to get all supported calling conventions that can be used on this architecture. All architectures support the :none calling convention which is a placeholder that cannot be used at runtime."
  [])

(defn ffi/close
  {:params [:abstract] :ret :nil}
  "(ffi/close native)\n\nFree a native object. Dereferencing pointers to symbols in the object will have undefined behavior after freeing."
  [native])

(defn ffi/context
  {:params [:string? :boolean? :boolean?] :ret :table}
  "(ffi/context &opt native-path &named map-symbols lazy)\n\nSet the path of the dynamic library to implicitly bind, as well     as other global state for ease of creating native bindings."
  [&opt native-path &named map-symbols lazy])

(defmacro ffi/defbind
  {:params [:symbol :any :any] :ret :function}
  "(ffi/defbind name ret-type & body)\n\nGenerate bindings for native functions in a convenient manner."
  [name ret-type & body])

(defmacro ffi/defbind-alias
  {:params [:symbol :symbol :any :any] :ret :function}
  "(ffi/defbind-alias name alias ret-type & body)\n\nGenerate bindings for native functions in a convenient manner.     Similar to defbind but allows for the janet function name to be     different than the FFI function."
  [name alias ret-type & body])

(defn ffi/free
  {:params [:pointer] :ret :nil}
  "(ffi/free pointer)\n\nFree memory allocated with `ffi/malloc`. Returns nil."
  [pointer])

(defn ffi/jitfn
  {:params [(or :string :buffer :symbol :keyword)] :ret :abstract}
  "(ffi/jitfn bytes)\n\nCreate an abstract type that can be used as the pointer argument to `ffi/call`. The content of `bytes` is architecture specific machine code that will be copied into executable memory."
  [bytes])

(defn ffi/lookup
  {:params [:abstract :string] :ret :pointer?}
  "(ffi/lookup native symbol-name)\n\nLookup a symbol from a native object. All symbol lookups will return a raw pointer if the symbol is found, else nil."
  [native symbol-name])

(defn ffi/malloc
  {:params [:number] :ret :pointer?}
  "(ffi/malloc size)\n\nAllocates memory directly using the janet memory allocator. Memory allocated in this way must be freed manually! Returns a raw pointer, or nil if size = 0."
  [size])

(defn ffi/native
  {:params [:string?] :ret :abstract}
  "(ffi/native &opt path)\n\nLoad a shared object or dll from the given path, and do not extract or run any code from it. This is different than `native`, which will run initialization code to get a module table. If `path` is nil, opens the current running binary. Returns a `core/native`."
  [&opt path])

(defn ffi/pointer-buffer
  {:params [:pointer :number :number? :number?] :ret :buffer}
  "(ffi/pointer-buffer pointer capacity &opt count offset)\n\nCreate a buffer from a pointer. The underlying memory of the buffer will not be reallocated or freed by the garbage collector, allowing unmanaged, mutable memory to be manipulated with buffer functions. Attempts to resize or extend the buffer beyond its initial capacity will raise an error. As with many FFI functions, this is memory unsafe and can potentially allow out of bounds memory access. Returns a new buffer."
  [pointer capacity &opt count offset])

(defn ffi/pointer-cfunction
  {:params [:pointer :string? :string? :number?] :ret :cfunction}
  "(ffi/pointer-cfunction pointer &opt name source-file source-line)\n\nCreate a C Function from a raw pointer. Optionally give the cfunction a name and source location for stack traces and debugging."
  [pointer &opt name source-file source-line])

(defn ffi/read
  {:params [CType (or :string :buffer :symbol :keyword) :number?] :ret :any} # TODO: the C source does not say what it returns
  "(ffi/read ffi-type bytes &opt offset)\n\nParse a native struct out of a buffer and convert it to normal Janet data structures. This function is the inverse of `ffi/write`. `bytes` can also be a raw pointer, although this is unsafe."
  [ffi-type bytes &opt offset])

(defn ffi/signature
  {:params [:keyword CType (or [CType] @[CType])] :ret :abstract}
  "(ffi/signature calling-convention ret-type & arg-types)\n\nCreate a function signature object that can be used to make calls with raw function pointers."
  [calling-convention ret-type & arg-types])

(defn ffi/size
  {:params [CType] :ret :number}
  "(ffi/size type)\n\nGet the size of an ffi type in bytes."
  [type])

(defn ffi/struct
  {:params [CType] :ret :abstract}
  "(ffi/struct & types)\n\nCreate a struct type definition that can be used to pass structs into native functions. "
  [& types])

(defn ffi/trampoline
  {:params [:keyword] :ret :pointer}
  "(ffi/trampoline cc)\n\nGet a native function pointer that can be used as a callback and passed to C libraries. This callback trampoline has the signature `void trampoline(void \\*ctx, void \\*userdata)` in the given calling convention. This is the only function signature supported. It is up to the programmer to ensure that the `userdata` argument contains a janet function the will be called with one argument, `ctx` which is an opaque pointer. This pointer can be further inspected with `ffi/read`."
  [cc])

(defn ffi/write
  {:params [CType :any :buffer? :number?] :ret :buffer}
  "(ffi/write ffi-type data &opt buffer index)\n\nAppend a native type to a buffer such as it would appear in memory. This can be used to pass pointers to structs in the ffi, or send C/C++/native structs over the network or to files. Returns a modified buffer or a new buffer if one is not supplied."
  [ffi-type data &opt buffer index])

(defmacro fiber-fn
  {:params [:keyword :any] :ret :fiber}
  "(fiber-fn flags & body)\n\nA wrapper for making fibers. Same as `(fiber/new (fn [] ;body) flags)`."
  [flags & body])

(defn fiber/can-resume?
  {:params [:fiber] :ret :boolean :narrows :any}
  "(fiber/can-resume? fiber)\n\nCheck if a fiber is finished and cannot be resumed."
  [fiber])

(defn fiber/current
  {:params [] :ret :fiber}
  "(fiber/current)\n\nReturns the currently running fiber."
  [])

(defn fiber/getenv
  {:params [:fiber] :ret :table?}
  "(fiber/getenv fiber)\n\nGets the environment for a fiber. Returns nil if no such table is set yet."
  [fiber])

(defn fiber/last-value
  {:params [:fiber] :ret :any} # TODO: the C source does not say what it returns
  "(fiber/last-value fiber)\n\nGet the last value returned or signaled from the fiber."
  [fiber])

(defn fiber/maxstack
  {:params [:fiber] :ret :number}
  "(fiber/maxstack fib)\n\nGets the maximum stack size in janet values allowed for a fiber. While memory for the fiber's stack is not allocated up front, the fiber will not allocated more than this amount and will throw a stack-overflow error if more memory is needed. "
  [fib])

(defn fiber/new
  {:params [:function (or :string :buffer :symbol :keyword) :table] :ret :fiber}
  "(fiber/new func &opt sigmask env)\n\nCreate a new fiber with function body func. Can optionally take a set of signals `sigmask` to capture from child fibers, and an environment table `env`. The mask is specified as a keyword where each character is used to indicate a signal to block. If the ev module is enabled, and this fiber is used as an argument to `ev/go`, these \"blocked\" signals will result in messages being sent to the supervisor channel. The default sigmask is :y. For example,\n\n    (fiber/new myfun :e123)\n\nblocks error signals and user signals 1, 2 and 3. The signals are as follows:\n\n* :a - block all signals\n* :d - block debug signals\n* :e - block error signals\n* :t - block termination signals: error + user[0-4]\n* :u - block user signals\n* :y - block yield signals\n* :w - block await signals (user9)\n* :r - block interrupt signals (user8)\n* :0-9 - block a specific user signal\n\nThe sigmask argument also can take environment flags. If any mutually exclusive flags are present, the last flag takes precedence.\n\n* :i - inherit the environment from the current fiber\n* :p - the environment table's prototype is the current environment table"
  [func &opt sigmask env])

(defn fiber/root
  {:params [] :ret :fiber}
  "(fiber/root)\n\nReturns the current root fiber. The root fiber is the oldest ancestor that does not have a parent. Note that a root fiber is also a task fiber."
  [])

(defn fiber/setenv
  {:params [:fiber :table] :ret :fiber}
  "(fiber/setenv fiber table)\n\nSets the environment table for a fiber. Set to nil to remove the current environment."
  [fiber table])

(defn fiber/setmaxstack
  {:params [:fiber :number] :ret :fiber}
  "(fiber/setmaxstack fib maxstack)\n\nSets the maximum stack size in janet values for a fiber. By default, the maximum stack size is usually 8192."
  [fib maxstack])

(defn fiber/status
  {:params [:fiber] :ret :keyword}
  "(fiber/status fib)\n\nGet the status of a fiber. The status will be one of:\n\n* :dead - the fiber has finished\n* :error - the fiber has errored out\n* :debug - the fiber is suspended in debug mode\n* :pending - the fiber has been yielded\n* :user(0-7) - the fiber is suspended by a user signal\n* :interrupted - the fiber was interrupted\n* :suspended - the fiber is waiting to be resumed by the scheduler\n* :alive - the fiber is currently running and cannot be resumed\n* :new - the fiber has just been created and not yet run"
  [fib])

(defn fiber?
  {:params [:any] :ret :boolean :narrows :fiber}
  "(fiber? x)\n\nCheck if x is a fiber."
  [x])

(defn file/close
  {:params [:abstract] :ret :nil}
  "(file/close f)\n\nClose a file and release all related resources. When you are done reading a file, close it to prevent a resource leak and let other processes read the file."
  [f])

(defn file/flush
  {:params [:abstract] :ret :abstract}
  "(file/flush f)\n\nFlush any buffered bytes to the file system. In most files, writes are buffered for efficiency reasons. Returns the file handle."
  [f])

(defn file/lines
  {:params [:abstract] :ret :fiber}
  "(file/lines file)\n\nReturn an iterator over the lines of a file."
  [file])

(defn file/open
  {:params [:string :keyword :number?] :ret :abstract?}
  "(file/open path &opt mode buffer-size)\n\nOpen a file. `path` is an absolute or relative path, and `mode` is a set of flags indicating the mode to open the file in. `mode` is a keyword where each character represents a flag. If the file cannot be opened, returns nil, otherwise returns the new file handle. Mode flags:\n\n* r - allow reading from the file\n\n* w - allow writing to the file\n\n* a - append to the file\n\nFollowing one of the initial flags, 0 or more of the following flags can be appended:\n\n* b - open the file in binary mode (rather than text mode)\n\n* + - append to the file instead of overwriting it\n\n* n - error if the file cannot be opened instead of returning nil\n\nSee fopen (<stdio.h>, C99) for further details."
  [path &opt mode buffer-size])

(defn file/read
  {:params [:abstract :number :buffer] :ret :buffer?}
  "(file/read f what &opt buf)\n\nRead a number of bytes from a file `f` into a buffer. A buffer `buf` can be provided as an optional third argument, otherwise a new buffer is created. `what` can either be an integer or a keyword. Returns the buffer with file contents. Values for `what`:\n\n* :all - read the whole file\n\n* :line - read up to and including the next newline character\n\n* n (integer) - read up to n bytes from the file"
  [f what &opt buf])

(defn file/seek
  {:params [:abstract :keyword :number] :ret :abstract}
  "(file/seek f &opt whence n)\n\nJump to a relative location in the file `f`. `whence` must be one of:\n\n* :cur - jump relative to the current file location\n\n* :set - jump relative to the beginning of the file\n\n* :end - jump relative to the end of the file\n\nBy default, `whence` is :cur. Optionally a value `n` may be passed for the relative number of bytes to seek in the file. `n` may be a real number to handle large files of more than 4GB. Returns the file handle."
  [f &opt whence n])

(defn file/tell
  {:params [:abstract] :ret :number}
  "(file/tell f)\n\nGet the current value of the file position for file `f`."
  [f])

(defn file/temp
  {:params [] :ret :abstract}
  "(file/temp)\n\nOpen an anonymous temporary file that is removed on close. Raises an error on failure."
  [])

(defn file/write
  {:params [:abstract (or :string :buffer)] :ret :abstract}
  "(file/write f & bytes)\n\nWrites to a file `f`. Each value of `bytes` must be a string, buffer, symbol, or keyword. Returns the file."
  [f & bytes])

(defn filewatch/add
  {:params [:abstract :string :keyword :keyword] :ret :abstract}
  "(filewatch/add watcher path flag & more-flags)\n\nAdd a path to the watcher. Available flags depend on the current OS, and are as follows:\n\nWindows/MINGW (flags correspond to `FILE_NOTIFY_CHANGE_*` flags in win32 documentation):\n\n* `:all` - trigger an event for all of the below triggers.\n\n* `:attributes` - `FILE_NOTIFY_CHANGE_ATTRIBUTES`\n\n* `:creation` - `FILE_NOTIFY_CHANGE_CREATION`\n\n* `:dir-name` - `FILE_NOTIFY_CHANGE_DIR_NAME`\n\n* `:last-access` - `FILE_NOTIFY_CHANGE_LAST_ACCESS`\n\n* `:last-write` - `FILE_NOTIFY_CHANGE_LAST_WRITE`\n\n* `:security` - `FILE_NOTIFY_CHANGE_SECURITY`\n\n* `:size` - `FILE_NOTIFY_CHANGE_SIZE`\n\n* `:recursive` - watch subdirectories recursively\n\nLinux (flags correspond to `IN_*` flags from <sys/inotify.h>):\n\n* `:access` - `IN_ACCESS`\n\n* `:all` - `IN_ALL_EVENTS`\n\n* `:attrib` - `IN_ATTRIB`\n\n* `:close-nowrite` - `IN_CLOSE_NOWRITE`\n\n* `:close-write` - `IN_CLOSE_WRITE`\n\n* `:create` - `IN_CREATE`\n\n* `:delete` - `IN_DELETE`\n\n* `:delete-self` - `IN_DELETE_SELF`\n\n* `:ignored` - `IN_IGNORED`\n\n* `:modify` - `IN_MODIFY`\n\n* `:move-self` - `IN_MOVE_SELF`\n\n* `:moved-from` - `IN_MOVED_FROM`\n\n* `:moved-to` - `IN_MOVED_TO`\n\n* `:open` - `IN_OPEN`\n\n* `:q-overflow` - `IN_Q_OVERFLOW`\n\n* `:unmount` - `IN_UNMOUNT`\n\n\nOn Windows, events will have the following possible types:\n\n* `:unknown`\n\n* `:added`\n\n* `:removed`\n\n* `:modified`\n\n* `:renamed-old`\n\n* `:renamed-new`\n\nOn Linux, events will have a `:type` corresponding to the possible flags, excluding `:all`.\n"
  [watcher path flag & more-flags])

(defn filewatch/listen
  {:params [:abstract] :ret :nil}
  "(filewatch/listen watcher)\n\nListen for changes in the watcher."
  [watcher])

(defn filewatch/new
  {:params [:abstract :keyword] :ret :abstract}
  "(filewatch/new channel & default-flags)\n\nCreate a new filewatcher that will give events to a channel channel. See `filewatch/add` for available flags.\n\nWhen an event is triggered by the filewatcher, a struct containing information will be given to channel as with `ev/give`. The contents of the channel depend on the OS, but will contain some common keys:\n\n* `:type` -- the type of the event that was raised.\n\n* `:file-name` -- the base file name of the file that triggered the event.\n\n* `:dir-name` -- the directory name of the file that triggered the event.\n\nEvents also will contain keys specific to the host OS.\n\nWindows has no extra properties on events.\n\nLinux has the following extra properties on events:\n\n* `:wd` -- the integer key returned by `filewatch/add` for the path that triggered this.\n\n* `:wd-path` -- the string path for watched directory of file. For files, will be the same as `:file-name`, and for directories, will be the same as `:dir-name`.\n\n* `:cookie` -- a randomized integer used to associate related events, such as :moved-from and :moved-to events.\n\n"
  [channel & default-flags])

(defn filewatch/remove
  {:params [:abstract :string] :ret :abstract}
  "(filewatch/remove watcher path)\n\nRemove a path from the watcher."
  [watcher path])

(defn filewatch/unlisten
  {:params [:abstract] :ret :nil}
  "(filewatch/unlisten watcher)\n\nStop listening for changes on a given watcher."
  [watcher])

(defn filter
  {:params [(fn [a] :any) (or [a] @[a])] :ret @[a]}
  "(filter pred ind)\n\nGiven a predicate, take only elements from an array or tuple for\nwhich `(pred element)` is truthy. Returns a new array."
  [pred ind])

(defn find
  {:params [(fn [a] :any) (or [a] @[a]) a?] :ret a?}
  "(find pred ind &opt dflt)\n\nFind the first value in an indexed collection that satisfies a predicate. Returns\n`dflt` if not found."
  [pred ind &opt dflt])

(defn find-index
  {:params [(fn [a] :any) (or [a] @[a]) :number?] :ret :number?}
  "(find-index pred ind &opt dflt)\n\nFind the index of indexed type for which `pred` is true. Returns `dflt` if not found."
  [pred ind &opt dflt])

(defn first
  {:params [(or [a] @[a])] :ret a?}
  "(first xs)\n\nGet the first element from an indexed data structure."
  [xs])

(defn flatten
  {:params [(or [:any] @[:any])] :ret @[:any]}
  "(flatten xs)\n\nTakes a nested array (tree) `xs` and returns the depth first traversal of\nit. Returns a new array."
  [xs])

(defn flatten-into
  {:params [@[:any] (or [:any] @[:any])] :ret @[:any]}
  "(flatten-into into xs)\n\nTakes a nested array (tree) `xs` and appends the depth first traversal of\n`xs` to array `into`. Returns `into`."
  [into xs])

(defn flush
  {:params [] :ret :nil}
  "(flush)\n\nFlush `(dyn :out stdout)` if it is a file, otherwise do nothing."
  [])

(defn flycheck
  {:params [:string :any] :ret :table}
  "(flycheck path &keys kwargs)\n\nCheck a file for errors without running the file. Found errors\nwill be printed to stderr in the usual format. Top level functions\nand macros that have the metadata `:flycheck` will also be evaluated\nduring flychecking. For full control, the `:flycheck` metadata can\nalso be a function that takes 4 arguments - `thunk`, `source`, `env`,\nand `where`, the same as the `:evaluator` argument to `run-context`.\nOther arguments to `flycheck` are the same as `dofile`. Returns nil."
  [path &keys kwargs])

(defmacro for
  {:params [:symbol :number :number :any] :ret :nil}
  "(for i start stop & body)\n\nDo a C-style for-loop for side effects. Returns nil."
  [i start stop & body])

(defmacro forever
  {:params [:any] :ret :nil}
  "(forever & body)\n\nEvaluate body forever in a loop, or until a break statement."
  [& body])

(defmacro forv
  {:params [:symbol :number :number :any] :ret :nil}
  "(forv i start stop & body)\n\nDo a C-style for-loop for side effects. The iteration variable `i`\ncan be mutated in the loop, unlike normal `for`. Returns nil."
  [i start stop & body])

(defn freeze
  {:params [:any] :ret :any}
  "(freeze x)\n\nFreeze an object (make it immutable) and do a deep copy, making\nchild values also immutable. Closures, fibers, and abstract types\nwill not be recursively frozen, but all other types will."
  [x])

(defn frequencies
  {:params [(or [a] @[a])] :ret @{:any :number}}
  "(frequencies ind)\n\nGet the number of occurrences of each value in an indexed data structure."
  [ind])

(defn from-pairs
  {:params [(or [[:any :any]] @[[:any :any]])] :ret :table}
  "(from-pairs ps)\n\nTakes a sequence of pairs and creates a table from each pair. It is the inverse of\n`pairs` on a table. Returns a new table."
  [ps])

(defn function?
  {:params [:any] :ret :boolean :narrows :function}
  "(function? x)\n\nCheck if x is a function (not a cfunction)."
  [x])

(defn gccollect
  {:params [] :ret :nil}
  "(gccollect)\n\nRun garbage collection. You should probably not call this manually."
  [])

(defn gcinterval
  {:params [] :ret :number}
  "(gcinterval)\n\nReturns the integer number of bytes to allocate before running an iteration of garbage collection."
  [])

(defn gcsetinterval
  {:params [:number] :ret :nil}
  "(gcsetinterval interval)\n\nSet an integer number of bytes to allocate before running garbage collection. Low values for interval will be slower but use less memory. High values will be faster but use more memory."
  [interval])

(defmacro generate
  {:params [:tuple :any] :ret :fiber}
  "(generate head & body)\n\nCreate a generator expression using the `loop` syntax. Returns a fiber\nthat yields all values inside the loop in order. See `loop` for details."
  [head & body])

(defn gensym
  {:params [] :ret :symbol}
  "(gensym)\n\nReturns a new symbol that is unique across the runtime. This means it will not collide with any already created symbols during compilation, so it can be used in macros to generate automatic bindings."
  [])

(defn geomean
  {:params [(or [:number] @[:number])] :ret :number}
  "(geomean xs)\n\nReturns the geometric mean of xs. If empty, returns NaN."
  [xs])

(defn get
  {:params [:any :any a] :ret a?}
  "(get ds key &opt dflt)\n\nGet the value mapped to key in data structure ds, and return dflt or nil if not found. Similar to in, but will not throw an error if the key is invalid for the data structure unless the data structure is an abstract type. In that case, the abstract type getter may throw an error."
  [ds key &opt dflt])

(defn get-in
  {:params [:any (or [:any] @[:any]) a] :ret a?}
  "(get-in ds ks &opt dflt)\n\nAccess a value in a nested data structure. Looks into the data structure via\na sequence of keys. If value is not found, and `dflt` is provided, returns `dflt`."
  [ds ks &opt dflt])

(defn getline
  {:params [:string :buffer (or :struct :table :nil)] :ret :buffer}
  "(getline &opt prompt buf env)\n\nReads a line of input into a buffer, including the newline character, using a prompt. An optional environment table can be provided for auto-complete. Returns the modified buffer. Use this function to implement a simple interface for a terminal program."
  [&opt prompt buf env])

(defn getproto
  {:params [:any] :ret (or :struct :table :nil)}
  "(getproto x)\n\nGet the prototype of a table or struct. Will return nil if `x` has no prototype."
  [x])

(defn group-by
  {:params [(fn [a] :any) (or [a] @[a])] :ret :table}
  "(group-by f ind)\n\nGroup elements of `ind` by a function `f` and put the results into a new table. The keys of\nthe table are the distinct return values from calling `f` on the elements of `ind`. The values\nof the table are arrays of all elements of `ind` for which `f` called on the element equals\nthat corresponding key."
  [f ind])

(defn has-key?
  {:params [:any :any] :ret :boolean :narrows :any}
  "(has-key? ds key)\n\nCheck if a data structure `ds` contains the key `key`."
  [ds key])

(defn has-value?
  {:params [:any :any] :ret :boolean :narrows :any}
  "(has-value? ds value)\n\nCheck if a data structure `ds` contains the value `value`. Will run in time proportional to the size of `ds`."
  [ds value])

(defn hash
  {:params [:any] :ret :number}
  "(hash value)\n\nGets a hash for any value. The hash is an integer can be used as a cheap hash function for all values. If two values are strictly equal, then they will have the same hash value."
  [value])

(defn idempotent?
  {:params [:any] :ret :boolean :narrows :any}
  "(idempotent? x)\n\nCheck if x is a value that evaluates to itself when compiled."
  [x])

(defn identity
  {:params [a] :ret a}
  "(identity x)\n\nA function that returns its argument."
  [x])

(defmacro if-let
  {:params [:tuple a b] :ret (or a b)}
  "(if-let bindings tru &opt fal)\n\nMake multiple bindings, and if all are truthy,\nevaluate the `tru` form. If any are false or nil, evaluate\nthe `fal` form. Bindings have the same syntax as the `let` macro."
  [bindings tru &opt fal])

(defmacro if-not
  {:params [:any a b] :ret (or a b)}
  "(if-not condition then &opt else)\n\nShorthand for `(if (not condition) else then)`."
  [condition then &opt else])

(defmacro if-with
  {:params [:tuple a b] :ret (or a b)}
  "(if-with [binding ctor dtor] truthy &opt falsey)\n\nSimilar to `with`, but if binding is false or nil, evaluates\nthe falsey path. Otherwise, evaluates the truthy path. In both cases,\n`ctor` is bound to binding."
  [[binding ctor dtor] truthy &opt falsey])

(defmacro import
  {:params [:any :any] :ret :nil}
  "(import path & args)\n\nImport a module. First requires the module, and then merges its\nsymbols into the current environment, prepending a given prefix as needed.\n(use the :as or :prefix option to set a prefix). If no prefix is provided,\nuse the name of the module as a prefix. One can also use \"`:export true`\"\nto re-export the imported symbols. If \"`:exit true`\" is given as an argument,\nany errors encountered at the top level in the module will cause `(os/exit 1)`\nto be called. Dynamic bindings will NOT be imported. Use :fresh with a truthy\nvalue to bypass the module cache. Use `:only [foo bar baz]` to only import\nselect bindings into the current environment."
  [path & args])

(defn import*
  {:params [:string :any] :ret :nil}
  "(import* path & args)\n\nFunction form of `import`. Same parameters, but the path\nand other symbol parameters should be strings instead."
  [path & args])

(defn in
  {:params [:any :any a] :ret a?}
  "(in ds key &opt dflt)\n\nGet value in ds at key, works on associative data structures. Arrays, tuples, tables, structs, strings, symbols, and buffers are all associative and can be used. Arrays, tuples, strings, buffers, and symbols must use integer keys that are in bounds or an error is raised. Structs and tables can take any value as a key except nil and will return nil or dflt if not found."
  [ds key &opt dflt])

(defn inc
  {:params [:number] :ret :number}
  "(inc x)\n\nReturns x + 1."
  [x])

(defn index-of
  {:params [a (or [a] @[a]) :number?] :ret :number?}
  "(index-of x ind &opt dflt)\n\nFind the first key associated with a value x in a data structure, acting like a reverse lookup.\nWill not look at table prototypes.\nReturns `dflt` if not found."
  [x ind &opt dflt])

(defn indexed?
  {:params [:any] :ret :boolean :narrows (or :tuple :array)}
  "(indexed? x)\n\nCheck if x is an array or tuple."
  [x])

(defn int/s64
  {:params [(or :number :string)] :ret :abstract}
  "(int/s64 value)\n\nCreate a boxed signed 64 bit integer from a string value or a number."
  [value])

(defn int/to-bytes
  {:params [:abstract :keyword? :buffer?] :ret :buffer}
  "(int/to-bytes value &opt endianness buffer)\n\nWrite the bytes of an `int/s64` or `int/u64` into a buffer.\nThe `buffer` parameter specifies an existing buffer to write to, if unset a new buffer will be created.\nReturns the modified buffer.\nThe `endianness` parameter indicates the byte order:\n- `nil` (unset): system byte order\n- `:le`: little-endian, least significant byte first\n- `:be`: big-endian, most significant byte first\n"
  [value &opt endianness buffer])

(defn int/to-number
  {:params [:abstract] :ret :number}
  "(int/to-number value)\n\nConvert an int/u64 or int/s64 to a number. Fails if the number is out of range for an int64."
  [value])

(defn int/u64
  {:params [(or :number :string)] :ret :abstract}
  "(int/u64 value)\n\nCreate a boxed unsigned 64 bit integer from a string value or a number."
  [value])

(defn int?
  {:params [:any] :ret :boolean :narrows :any}
  "(int? x)\n\nCheck if x can be exactly represented as a 32 bit signed two's complement integer."
  [x])

(defn interleave
  {:params [(or [:any] @[:any])] :ret @[:any]}
  "(interleave & cols)\n\nReturns an array of the first elements of each col, then the second elements, etc."
  [& cols])

(defn interpose
  {:params [a (or [a] @[a])] :ret @[a]}
  "(interpose sep ind)\n\nReturns a sequence of the elements of `ind` separated by\n`sep`. Returns a new array."
  [sep ind])

(defn invert
  {:params [(or :struct :table)] :ret :table}
  "(invert ds)\n\nGiven an associative data structure `ds`, returns a new table where the\nkeys of `ds` are the values, and the values are the keys. If multiple keys\nin `ds` are mapped to the same value, only one of those values will\nbecome a key in the returned table."
  [ds])

(def janet/build
  {:type :string}
  "The build identifier of the running janet program."
  nil)

(def janet/config-bits
  {:type :number}
  "The flag set of config options from janetconf.h which is used to check if native modules are compatible with the host program."
  nil)

(def janet/version
  {:type :string}
  "The version number of the running janet program."
  nil)

(defmacro juxt
  {:params [:any] :ret :function}
  "(juxt & funs)\n\nMacro form of `juxt*`. Same behavior but more efficient."
  [& funs])

(defn juxt*
  {:params [:function] :ret :function}
  "(juxt* & funs)\n\nReturns the juxtaposition of functions. In other words,\n`((juxt* a b c) x)` evaluates to `[(a x) (b x) (c x)]`."
  [& funs])

(defn keep
  {:params [(fn [a] b) (or [a] @[a]) (or [:any] @[:any])] :ret @[b]}
  "(keep pred ind & inds)\n\nGiven a predicate `pred`, return a new array containing the\ntruthy results of applying `pred` to each value in the data\nstructure `ind`, but only if no `inds` are provided. Multiple\ndata structures can be handled if each `inds` is a data\nstructure and `pred` is a function of arity one more than the\nnumber of `inds`. The resulting array has a length that is no\nlonger than the shortest of `ind` and each of `inds`."
  [pred ind & inds])

(defn keep-syntax
  {:params [:any a] :ret a}
  "(keep-syntax before after)\n\nCreates a tuple with the tuple type and sourcemap of `before` but the\nelements of `after`. If either one of its arguments is not a tuple, returns\n`after` unmodified. Useful to preserve syntactic information when transforming\nan ast in macros."
  [before after])

(defn keep-syntax!
  {:params [:any a] :ret a}
  "(keep-syntax! before after)\n\nLike `keep-syntax`, but if `after` is an array, it is coerced into a tuple.\nUseful to preserve syntactic information when transforming an ast in macros."
  [before after])

(defn keys
  {:params [:any] :ret @[:any]}
  "(keys x)\n\nGet the keys of an associative data structure."
  [x])

(defn keyword
  {:params [:any] :ret :keyword}
  "(keyword & xs)\n\nCreates a keyword by concatenating the elements of `xs` together. If an element is not a byte sequence, it is converted to bytes via `describe`. Returns the new keyword."
  [& xs])

(defn keyword/slice
  {:params [(or :string :buffer :symbol :keyword) :number? :number?] :ret :keyword}
  "(keyword/slice bytes &opt start end)\n\nSame as string/slice, but returns a keyword."
  [bytes &opt start end])

(defn keyword?
  {:params [:any] :ret :boolean :narrows :keyword}
  "(keyword? x)\n\nCheck if x is a keyword."
  [x])

(defn kvs
  {:params [(or :struct :table)] :ret @[:any]}
  "(kvs dict)\n\nTakes a table or struct and returns a new array of key value pairs\nlike `@[k v k v ...]`."
  [dict])

(defmacro label
  {:params [:symbol :any] :ret :any}
  "(label name & body)\n\nSet a label point that is lexically scoped. `name` should be a symbol\nthat will be bound to the label."
  [name & body])

(defn last
  {:params [(or [a] @[a])] :ret a?}
  "(last xs)\n\nGet the last element from an indexed data structure."
  [xs])

(defn length
  {:params [:any] :ret :number}
  "(length ds)\n\nReturns the length or count of a data structure in constant time as an integer. For structs and tables, returns the number of key-value pairs in the data structure."
  [ds])

(defn lengthable?
  {:params [:any] :ret :boolean :narrows :any}
  "(lengthable? x)\n\nCheck if x is a bytes, indexed, or dictionary."
  [x])

(defmacro let
  {:params [:tuple :any] :ret :any}
  "(let bindings & body)\n\nCreate a scope and bind values to symbols. Each pair in `bindings` is\nassigned as if with `def`, and the body of the `let` form returns the last\nvalue."
  [bindings & body])

(defn load-image
  {:params [(or :string :buffer)] :ret :table}
  "(load-image image)\n\nThe inverse operation to `make-image`. Returns an environment."
  [image])

(def load-image-dict
  {:type :table}
  "A table used in combination with `unmarshal` to unmarshal byte sequences created\nby `make-image`, such that `(load-image bytes)` is the same as `(unmarshal bytes load-image-dict)`."
  nil)

(defmacro loop
  {:params [:tuple :any] :ret :nil}
  "(loop head & body)\n\nA general purpose loop macro. This macro is similar to the Common Lisp loop\nmacro, although intentionally much smaller in scope.  The head of the loop\nshould be a tuple that contains a sequence of either bindings or\nconditionals. A binding is a sequence of three values that define something\nto loop over. Bindings are written in the format:\n\n    binding :verb object/expression\n\nwhere `binding` is a binding as passed to def, `:verb` is one of a set of\nkeywords, and `object` is any expression. Each subsequent binding creates a\nnested loop within the loop created by the previous binding.\n\nThe available verbs are:\n\n* `:iterate` -- repeatedly evaluate and bind to the expression while it is\n  truthy.\n\n* `:range` -- loop over a range. The object should be a two-element tuple with\n  a start and end value, and an optional positive step. The range is half\n  open, [start, end).\n\n* `:range-to` -- same as :range, but the range is inclusive [start, end].\n\n* `:down` -- loop over a range, stepping downwards. The object should be a\n  two-element tuple with a start and (exclusive) end value, and an optional\n  (positive!) step size.\n\n* `:down-to` -- same as :down, but the range is inclusive [start, end].\n\n* `:keys` -- iterate over the keys in a data structure.\n\n* `:pairs` -- iterate over the key-value pairs as tuples in a data structure.\n\n* `:in` -- iterate over the values in a data structure or fiber.\n\n`loop` also accepts conditionals to refine the looping further. Conditionals are of\nthe form:\n\n    :modifier argument\n\nwhere `:modifier` is one of a set of keywords, and `argument` is keyword-dependent.\n`:modifier` can be one of:\n\n* `:while expression` -- breaks from the current loop if `expression` is\n  falsey.\n\n* `:until expression` -- breaks from the current loop if `expression` is\n  truthy.\n\n* `:let bindings` -- defines bindings inside the current loop as passed to the\n  `let` macro.\n\n* `:before form` -- evaluates a form for a side effect before the next inner\n  loop.\n\n* `:after form` -- same as `:before`, but the side effect happens after the\n  next inner loop.\n\n* `:repeat n` -- repeats the next inner loop `n` times.\n\n* `:when condition` -- only evaluates the current loop body when `condition`\n  is truthy.\n\n* `:unless condition` -- only evaluates the current loop body when `condition`\n  is falsey.\n\nThe `loop` macro always evaluates to nil."
  [head & body])

(defn macex
  {:params [:any :function?] :ret :any}
  "(macex x &opt on-binding)\n\nExpand macros completely.\n`on-binding` is an optional callback for whenever a normal symbolic binding\nis encountered. This allows macros to easily see all bindings used by their\narguments by calling `macex` on their contents. The binding itself is also\nreplaced by the value returned by `on-binding` within the expanded macro."
  [x &opt on-binding])

(defn macex1
  {:params [:any :function?] :ret :any}
  "(macex1 x &opt on-binding)\n\nExpand macros in a form, but do not recursively expand macros.\nSee `macex` docs for info on `on-binding`."
  [x &opt on-binding])

(defn maclintf
  {:params [:keyword :string :any] :ret :nil}
  "(maclintf level fmt & args)\n\nWhen inside a macro, call this function to add a linter warning. Takes\na `fmt` argument like `string/format`, which is used to format the message."
  [level fmt & args])

(defn make-env
  {:params [(or :struct :table :nil)] :ret :table}
  "(make-env &opt parent)\n\nCreate a new environment table. The new environment\nwill inherit bindings from the parent environment, but new\nbindings will not pollute the parent environment."
  [&opt parent])

(defn make-image
  {:params [(or :struct :table)] :ret :buffer}
  "(make-image env)\n\nCreate an image from an environment returned by `require`.\nReturns the image source as a string."
  [env])

(def make-image-dict
  {:type :table}
  "A table used in combination with `marshal` to marshal code (images), such that\n`(make-image x)` is the same as `(marshal x make-image-dict)`."
  nil)

(defn map
  {:params [(fn [a] b) (or [a] @[a]) (or [:any] @[:any])] :ret @[b]}
  "(map f ind & inds)\n\nMap a function `f` over every value in a data structure `ind`\nand return an array of results, but only if no `inds` are\nprovided. Multiple data structures can be handled if each\n`inds` is a data structure and `f` is a function of arity\none more than the number of `inds`.  The resulting array has\na length that is the shortest of `ind` and each of `inds`."
  [f ind & inds])

(defn mapcat
  {:params [(fn [a] (or [b] @[b])) (or [a] @[a]) (or [:any] @[:any])] :ret @[b]}
  "(mapcat f ind & inds)\n\nMap a function `f` over every value in a data structure `ind`\nand use `array/concat` to concatenate the results, but only if\nno `inds` are provided. Multiple data structures can be handled\nif each `inds` is a data structure and `f` is a function of\narity one more than the number of `inds`. Note that `f` is only\napplied to values at indeces up to the largest index of the\nshortest of `ind` and each of `inds`."
  [f ind & inds])

(defn marshal
  {:params [:any :table :buffer :any] :ret :buffer}
  "(marshal x &opt reverse-lookup buffer no-cycles)\n\nMarshal a value into a buffer and return the buffer. The buffer can then later be unmarshalled to reconstruct the initial value. Optionally, one can pass in a reverse lookup table to not marshal aliased values that are found in the table. Then a forward lookup table can be used to recover the original value when unmarshalling."
  [x &opt reverse-lookup buffer no-cycles])

(defmacro match
  {:params [:any :any] :ret :any}
  "(match x & cases)\n\nPattern matching. Match an expression `x` against any number of cases.\nEach case is a pattern to match against, followed by an expression to\nevaluate to if that case is matched.  Legal patterns are:\n\n* symbol -- a pattern that is a symbol will match anything, binding `x`'s\n  value to that symbol.\n\n* array or bracket tuple -- an array or bracket tuple will match only if\n  all of its elements match the corresponding elements in `x`.\n  Use `& rest` at the end of an array or bracketed tuple to bind all remaining values to `rest`.\n\n* table or struct -- a table or struct will match if all values match with\n  the corresponding values in `x`.\n\n* tuple -- a tuple pattern will match if its first element matches, and the\n  following elements are treated as predicates and are true.\n\n* `_` symbol -- the last special case is the `_` symbol, which is a wildcard\n  that will match any value without creating a binding.\n\nWhile a symbol pattern will ordinarily match any value, the pattern `(@ <sym>)`,\nwhere `<sym>` is any symbol, will attempt to match `x` against a value\nalready bound to `<sym>`, rather than matching and rebinding it.\n\nAny other value pattern will only match if it is equal to `x`.\nQuoting a pattern with `'` will also treat the value as a literal value to match against.\n"
  [x & cases])

(def math/-inf
  {:type :number}
  "The number representing negative infinity"
  nil)

(defn math/abs
  {:params [:number] :ret :number}
  "(math/abs x)\n\nReturn the absolute value of x."
  [x])

(defn math/acos
  {:params [:number] :ret :number}
  "(math/acos x)\n\nReturns the arccosine of x."
  [x])

(defn math/acosh
  {:params [:number] :ret :number}
  "(math/acosh x)\n\nReturns the hyperbolic arccosine of x."
  [x])

(defn math/asin
  {:params [:number] :ret :number}
  "(math/asin x)\n\nReturns the arcsin of x."
  [x])

(defn math/asinh
  {:params [:number] :ret :number}
  "(math/asinh x)\n\nReturns the hyperbolic arcsine of x."
  [x])

(defn math/atan
  {:params [:number] :ret :number}
  "(math/atan x)\n\nReturns the arctangent of x."
  [x])

(defn math/atan2
  {:params [:number :number] :ret :number}
  "(math/atan2 y x)\n\nReturns the arctangent of y/x. Works even when x is 0."
  [y x])

(defn math/atanh
  {:params [:number] :ret :number}
  "(math/atanh x)\n\nReturns the hyperbolic arctangent of x."
  [x])

(defn math/cbrt
  {:params [:number] :ret :number}
  "(math/cbrt x)\n\nReturns the cube root of x."
  [x])

(defn math/ceil
  {:params [:number] :ret :number}
  "(math/ceil x)\n\nReturns the smallest integer value number that is not less than x."
  [x])

(defn math/cos
  {:params [:number] :ret :number}
  "(math/cos x)\n\nReturns the cosine of x."
  [x])

(defn math/cosh
  {:params [:number] :ret :number}
  "(math/cosh x)\n\nReturns the hyperbolic cosine of x."
  [x])

(def math/e
  {:type :number}
  "The base of the natural log."
  nil)

(defn math/erf
  {:params [:number] :ret :number}
  "(math/erf x)\n\nReturns the error function of x."
  [x])

(defn math/erfc
  {:params [:number] :ret :number}
  "(math/erfc x)\n\nReturns the complementary error function of x."
  [x])

(defn math/exp
  {:params [:number] :ret :number}
  "(math/exp x)\n\nReturns e to the power of x."
  [x])

(defn math/exp2
  {:params [:number] :ret :number}
  "(math/exp2 x)\n\nReturns 2 to the power of x."
  [x])

(defn math/expm1
  {:params [:number] :ret :number}
  "(math/expm1 x)\n\nReturns e to the power of x minus 1."
  [x])

(defn math/floor
  {:params [:number] :ret :number}
  "(math/floor x)\n\nReturns the largest integer value number that is not greater than x."
  [x])

(defn math/frexp
  {:params [:number] :ret :tuple}
  "(math/frexp x)\n\nReturns a tuple of (mantissa, exponent) from number."
  [x])

(defn math/gamma
  {:params [:number] :ret :number}
  "(math/gamma x)\n\nReturns gamma(x)."
  [x])

(defn math/gcd
  {:params [:number :number] :ret :number}
  "(math/gcd x y)\n\nReturns the greatest common divisor between x and y."
  [x y])

(defn math/hypot
  {:params [:number :number] :ret :number}
  "(math/hypot a b)\n\nReturns c from the equation c^2 = a^2 + b^2."
  [a b])

(def math/inf
  {:type :number}
  "The number representing positive infinity"
  nil)

(def math/int-max
  {:type :number}
  "The maximum contiguous integer representable by a double (2^53)"
  nil)

(def math/int-min
  {:type :number}
  "The minimum contiguous integer representable by a double (-(2^53))"
  nil)

(def math/int32-max
  {:type :number}
  "The maximum contiguous integer representable by a 32 bit signed integer"
  nil)

(def math/int32-min
  {:type :number}
  "The minimum contiguous integer representable by a 32 bit signed integer"
  nil)

(defn math/lcm
  {:params [:number :number] :ret :number}
  "(math/lcm x y)\n\nReturns the least common multiple of x and y."
  [x y])

(defn math/ldexp
  {:params [:number :number] :ret :number}
  "(math/ldexp m e)\n\nCreates a new number from a mantissa and an exponent."
  [m e])

(defn math/log
  {:params [:number] :ret :number}
  "(math/log x)\n\nReturns the natural logarithm of x."
  [x])

(defn math/log-gamma
  {:params [:number] :ret :number}
  "(math/log-gamma x)\n\nReturns log-gamma(x)."
  [x])

(defn math/log10
  {:params [:number] :ret :number}
  "(math/log10 x)\n\nReturns the log base 10 of x."
  [x])

(defn math/log1p
  {:params [:number] :ret :number}
  "(math/log1p x)\n\nReturns (log base e of x) + 1 more accurately than (+ (math/log x) 1)"
  [x])

(defn math/log2
  {:params [:number] :ret :number}
  "(math/log2 x)\n\nReturns the log base 2 of x."
  [x])

(def math/nan
  {:type :number}
  "Not a number (IEEE-754 NaN)"
  nil)

(defn math/next
  {:params [:number :number] :ret :number}
  "(math/next x y)\n\nReturns the next representable floating point value after x in the direction of y."
  [x y])

(def math/pi
  {:type :number}
  "The value pi."
  nil)

(defn math/pow
  {:params [:number :number] :ret :number}
  "(math/pow a x)\n\nReturns a to the power of x."
  [a x])

(defn math/random
  {:params [] :ret :number}
  "(math/random)\n\nReturns a uniformly distributed random number between 0 and 1."
  [])

(defn math/rng
  {:params [:number?] :ret :abstract}
  "(math/rng &opt seed)\n\nCreates a Pseudo-Random number generator, with an optional seed. The seed should be an unsigned 32 bit integer or a buffer. Do not use this for cryptography. Returns a core/rng abstract type."
  [&opt seed])

(defn math/rng-buffer
  {:params [:abstract :number :buffer?] :ret :buffer}
  "(math/rng-buffer rng n &opt buf)\n\nGet n random bytes and put them in a buffer. Creates a new buffer if no buffer is provided, otherwise appends to the given buffer. Returns the buffer."
  [rng n &opt buf])

(defn math/rng-int
  {:params [:abstract :number?] :ret :number}
  "(math/rng-int rng &opt max)\n\nExtract a random integer in the range [0, max) for max > 0 from the RNG.  If max is 0, return 0.  If no max is given, the default is 2^31 - 1."
  [rng &opt max])

(defn math/rng-uniform
  {:params [:abstract] :ret :number}
  "(math/rng-uniform rng)\n\nExtract a random number in the range [0, 1) from the RNG."
  [rng])

(defn math/round
  {:params [:number] :ret :number}
  "(math/round x)\n\nReturns the integer nearest to x."
  [x])

(defn math/seedrandom
  {:params [(or :string :buffer :number)] :ret :nil}
  "(math/seedrandom seed)\n\nSet the seed for the random number generator. `seed` should be an integer or a buffer."
  [seed])

(defn math/sin
  {:params [:number] :ret :number}
  "(math/sin x)\n\nReturns the sine of x."
  [x])

(defn math/sinh
  {:params [:number] :ret :number}
  "(math/sinh x)\n\nReturns the hyperbolic sine of x."
  [x])

(defn math/sqrt
  {:params [:number] :ret :number}
  "(math/sqrt x)\n\nReturns the square root of x."
  [x])

(defn math/tan
  {:params [:number] :ret :number}
  "(math/tan x)\n\nReturns the tangent of x."
  [x])

(defn math/tanh
  {:params [:number] :ret :number}
  "(math/tanh x)\n\nReturns the hyperbolic tangent of x."
  [x])

(defn math/trunc
  {:params [:number] :ret :number}
  "(math/trunc x)\n\nReturns the integer between x and 0 nearest to x."
  [x])

(defn max
  {:params [:number] :ret :number}
  "(max & args)\n\nReturns the numeric maximum of the arguments."
  [& args])

(defn max-of
  {:params [(or [:number] @[:number])] :ret :number}
  "(max-of args)\n\nReturns the numeric maximum of the argument sequence."
  [args])

(defn mean
  {:params [(or [:number] @[:number])] :ret :number}
  "(mean xs)\n\nReturns the mean of xs. If empty, returns NaN."
  [xs])

(defn memcmp
  {:params [(or :string :buffer :symbol :keyword) (or :string :buffer :symbol :keyword) :number? :number? :number?] :ret :number}
  "(memcmp a b &opt len offset-a offset-b)\n\nCompare memory. Takes two byte sequences `a` and `b`, and return 0 if they have identical contents, a negative integer if a is less than b, and a positive integer if a is greater than b. Optionally take a length and offsets to compare slices of the bytes sequences."
  [a b &opt len offset-a offset-b])

(defn merge
  {:params [(or :struct :table)] :ret :table}
  "(merge & colls)\n\nMerges multiple tables/structs into one new table. If a key appears in more than one\ncollection in `colls`, then later values replace any previous ones.\nReturns the new table."
  [& colls])

(defn merge-into
  {:params [:table (or :struct :table)] :ret :table}
  "(merge-into tab & colls)\n\nMerges multiple tables/structs into table `tab`. If a key appears in more than one\ncollection in `colls`, then later values replace any previous ones. Returns `tab`."
  [tab & colls])

(defn merge-module
  {:params [:table (or :struct :table) :string? :boolean? :any] :ret :table}
  "(merge-module target source &opt prefix export only)\n\nMerge a module source into the `target` environment with a `prefix`, as with the `import` macro.\nThis lets users emulate the behavior of `import` with a custom module table.\nIf `export` is truthy, then merged functions are not marked as private. Returns\nthe modified target environment. If a tuple or array `only` is passed, only merge keys in `only`."
  [target source &opt prefix export only])

(defn min
  {:params [:number] :ret :number}
  "(min & args)\n\nReturns the numeric minimum of the arguments."
  [& args])

(defn min-of
  {:params [(or [:number] @[:number])] :ret :number}
  "(min-of args)\n\nReturns the numeric minimum of the argument sequence."
  [args])

(defn mod
  {:params [:number] :ret :number}
  "(mod & xs)\n\nReturns the result of applying the modulo operator on the first value of xs with each remaining value. `(mod x 0)` is defined to be `x`."
  [& xs])

(defn module/add-file-extension
  {:params [:string :keyword] :ret :array}
  "(module/add-file-extension ext loader)\n\nAdd paths to `module/paths` for a given file extension such that\nthe programmer can import a module by relative or absolute path from\nthe current working directory.\nReturns the modified `module/paths`."
  [ext loader])

(defn module/add-paths
  {:params [:string :keyword] :ret :array}
  "(module/add-paths ext loader)\n\nAdd paths to `module/paths` for a given loader such that\nthe generated paths behave like other module types, including\nrelative imports and syspath imports. `ext` is the file extension\nto associate with this module type, including the dot. `loader` is the\nkeyword name of a loader in `module/loaders`. The parameter `match-exact-path`\nwill allow users to import files with this extension directly with a relative\nor absolute path. Returns the modified `module/paths`."
  [ext loader])

(defn module/add-syspath
  {:params [:string] :ret :array}
  "(module/add-syspath path)\n\nAdd a custom syspath to `module/paths` by duplicating all entries that being with `:sys:` and\nadding duplicates with a specific path prefix instead."
  [path])

(def module/cache
  {:type :table}
  "A table, mapping loaded module identifiers to their environments."
  nil)

(defn module/expand-path
  {:params [:string :string] :ret :buffer}
  "(module/expand-path path template)\n\nExpands a path template as found in `module/paths` for `module/find`. This takes in a path (the argument to require) and a template string, to expand the path to a path that can be used for importing files. The replacements are as follows:\n\n* :all: -- the value of path verbatim.\n\n* :@all: -- Same as :all:, but if `path` starts with the @ character, the first path segment is replaced with a dynamic binding `(dyn <first path segment as keyword>)`.\n\n* :cur: -- the directory portion, if any, of (dyn :current-file)\n\n* :dir: -- the directory portion, if any, of the path argument\n\n* :name: -- the name component of path, with extension if given\n\n* :native: -- the extension used to load natives, .so or .dll\n\n* :sys: -- the system path, or (dyn :syspath)"
  [path template])

(defn module/find
  {:params [:string :any] :ret [:any :any]}
  "(module/find path &opt find-all)\n\nTry to match a module or path name from the patterns in `module/paths`.\nReturns a tuple (fullpath kind) where the kind is one of :source, :native,\nor :image if the module is found, otherwise a tuple with nil followed by\nan error message."
  [path &opt find-all])

(def module/loaders
  {:type :table}
  "A table of loading method names to loading functions.\nThis table lets `require` and `import` load many different kinds\nof files as modules."
  nil)

(def module/loading
  {:type :table}
  "A table, mapping currently loading modules to true. Used to prevent\ncircular dependencies."
  nil)

(def module/paths
  {:type :array}
  "The list of paths to look for modules, templated for `module/expand-path`.\nEach element is a two-element tuple, containing the path\ntemplate and a keyword :source, :native, or :image indicating how\n`require` should load files found at these paths.\n\nA tuple can also\ncontain a third element, specifying a filter that prevents `module/find`\nfrom searching that path template if the filter doesn't match the input\npath. The filter can be a string or a predicate function, and\nis often a file extension, including the period."
  nil)

(defn module/value
  {:params [(or :struct :table) :symbol :boolean?] :ret :any}
  "(module/value module sym &opt private)\n\nGiven a module table, get the value bound to a symbol `sym`. If `private` is\ntruthy, will also resolve private module symbols. If no binding is found, will return\nnil."
  [module sym &opt private])

(defn nan?
  {:params [:any] :ret :boolean :narrows :any}
  "(nan? x)\n\nCheck if x is NaN."
  [x])

(defn nat?
  {:params [:any] :ret :boolean :narrows :any}
  "(nat? x)\n\nCheck if x can be exactly represented as a non-negative 32 bit signed two's complement integer."
  [x])

(defn native
  {:params [:string :table] :ret :table}
  "(native path &opt env)\n\nLoad a native module from the given path. The path must be an absolute or relative path on the file system, and is usually a .so file on Unix systems, and a .dll file on Windows. Returns an environment table that contains functions and other values from the native module."
  [path &opt env])

(defn neg?
  {:params [:number] :ret :boolean :narrows :any}
  "(neg? x)\n\nCheck if x is less than 0."
  [x])

(defn net/accept
  {:params [:abstract :number?] :ret (or :abstract :nil)}
  "(net/accept stream &opt timeout)\n\nGet the next connection on a server stream. This would usually be called in a loop in a dedicated fiber. Takes an optional timeout in seconds, after which will raise an error. Returns a new duplex stream which represents a connection to the client."
  [stream &opt timeout])

(defn net/accept-loop
  {:params [:abstract :function] :ret :nil}
  "(net/accept-loop stream handler)\n\nShorthand for running a server stream that will continuously accept new connections. Blocks the current fiber until the stream is closed, and will return the stream."
  [stream handler])

(defn net/address
  {:params [:string (or :string :number) :keyword? :boolean?] :ret (or :string @[:string])}
  "(net/address host port &opt type multi)\n\nLook up the connection information for a given hostname, port, and connection type. Returns a handle that can be used to send datagrams over network without establishing a connection. On Posix platforms, you can use :unix for host to connect to a unix domain socket, where the name is given in the port argument. On Linux, abstract unix domain sockets are specified with a leading '@' character in port. If `multi` is truthy, will return all address that match in an array instead of just the first."
  [host port &opt type multi])

(defn net/address-unpack
  {:params [:abstract] :ret :tuple}
  "(net/address-unpack address)\n\nGiven an address returned by net/address, return a host, port pair. Unix domain sockets will have only the path in the returned tuple."
  [address])

(defn net/chunk
  {:params [:abstract :number :buffer? :number?] :ret (or :buffer :nil)}
  "(net/chunk stream nbytes &opt buf timeout)\n\nSame a net/read, but will wait for all n bytes to arrive rather than return early. Takes an optional timeout in seconds, after which will raise an error."
  [stream nbytes &opt buf timeout])

(defn net/close
  {:params [:abstract] :ret :nil}
  "(net/close stream)\n\nAlias for `ev/close`."
  [stream])

(defn net/connect
  {:params [:string (or :string :number) :keyword? :string? :string?] :ret :abstract}
  "(net/connect host port &opt type bindhost bindport)\n\nOpen a connection to communicate with a server. Returns a duplex stream that can be used to communicate with the server. Type is an optional keyword to specify a connection type, either :stream or :datagram. The default is :stream. Bindhost is an optional string to select from what address to make the outgoing connection, with the default being the same as using the OS's preferred address. "
  [host port &opt type bindhost bindport])

(defn net/flush
  {:params [:abstract] :ret :abstract}
  "(net/flush stream)\n\nMake sure that a stream is not buffering any data. This temporarily disables Nagle's algorithm. Use this to make sure data is sent without delay. Returns stream."
  [stream])

(defn net/listen
  {:params [:string (or :string :number) :keyword? :boolean?] :ret :abstract}
  "(net/listen host port &opt type no-reuse)\n\nCreates a server. Returns a new stream that is neither readable nor writeable. Use net/accept or net/accept-loop be to handle connections and start the server. The type parameter specifies the type of network connection, either a :stream (usually tcp), or :datagram (usually udp). If not specified, the default is :stream. The host and port arguments are the same as in net/address. The last boolean parameter `no-reuse` will disable the use of `SO_REUSEADDR` and `SO_REUSEPORT` when creating a server on some operating systems."
  [host port &opt type no-reuse])

(defn net/localname
  {:params [:abstract] :ret :tuple}
  "(net/localname stream)\n\nGets the local address and port in a tuple in that order."
  [stream])

(defn net/peername
  {:params [:abstract] :ret :tuple}
  "(net/peername stream)\n\nGets the remote peer's address and port in a tuple in that order."
  [stream])

(defn net/read
  {:params [:abstract :number :buffer? :number?] :ret (or :buffer :nil)}
  "(net/read stream nbytes &opt buf timeout)\n\nRead up to n bytes from a stream, suspending the current fiber until the bytes are available. `n` can also be the keyword `:all` to read into the buffer until end of stream. If less than n bytes are available (and more than 0), will push those bytes and return early. Takes an optional timeout in seconds, after which will raise an error. Returns a buffer with up to n more bytes in it, or raises an error if the read failed."
  [stream nbytes &opt buf timeout])

(defn net/recv-from
  {:params [:abstract :number :buffer :number?] :ret (or :buffer :nil)}
  "(net/recv-from stream nbytes buf &opt timeout)\n\nReceives data from a server stream and puts it into a buffer. Returns the socket-address the packet came from. Takes an optional timeout in seconds, after which will raise an error."
  [stream nbytes buf &opt timeout])

(defn net/send-to
  {:params [:abstract :abstract (or :string :buffer) :number?] :ret :abstract}
  "(net/send-to stream dest data &opt timeout)\n\nWrites a datagram to a server stream. dest is a the destination address of the packet. Takes an optional timeout in seconds, after which will raise an error. Returns stream."
  [stream dest data &opt timeout])

(defn net/server
  {:params [:string (or :string :number) :function? :keyword? :boolean?] :ret :abstract}
  "(net/server host port &opt handler type no-reuse)\n\nStarts a server with `net/listen`. Runs `net/accept-loop` asynchronously if\n`handler` is set and `type` is `:stream` (the default). It is invalid to set\n`handler` if `type` is `:datagram`. Returns the new server stream."
  [host port &opt handler type no-reuse])

(defn net/setsockopt
  {:params [:abstract :keyword :any] :ret :nil} # TODO: the C source does not say what value is
  "(net/setsockopt stream option value)\n\nset socket options.\n\nsupported options and associated value types:\n- :so-broadcast boolean\n- :so-reuseaddr boolean\n- :so-keepalive boolean\n- :ip-multicast-ttl number\n- :ip-add-membership string\n- :ip-drop-membership string\n- :ipv6-join-group string\n- :ipv6-leave-group string\n- :ipv6-multicast-hops number\n- :ipv6-unicast-hops number\n"
  [stream option value])

(defn net/shutdown
  {:params [:abstract :keyword] :ret :abstract}
  "(net/shutdown stream &opt mode)\n\nStop communication on this socket in a graceful manner, either in both directions or just reading/writing from the stream. The `mode` parameter controls which communication to stop on the socket. \n\n* `:wr` is the default and prevents both reading new data from the socket and writing new data to the socket.\n* `:r` disables reading new data from the socket.\n* `:w` disable writing data to the socket.\n\nReturns the original socket."
  [stream &opt mode])

(defn net/socket
  {:params [:keyword :keyword] :ret :abstract}
  "(net/socket &opt type address-family)\n\nCreates a new unbound socket. Type is an optional keyword, either a :stream (usually tcp), or :datagram (usually udp). The default is :stream. `address-family` should be one of :ipv4 or :ipv6."
  [&opt type address-family])

(defn net/write
  {:params [:abstract (or :string :buffer) :number?] :ret :nil}
  "(net/write stream data &opt timeout)\n\nWrite data to a stream, suspending the current fiber until the write completes. Takes an optional timeout in seconds, after which will raise an error. Returns nil, or raises an error if the write failed."
  [stream data &opt timeout])

(defn next
  {:params [:any :any] :ret :any}
  "(next ds &opt key)\n\nGets the next key in a data structure. Can be used to iterate through the keys of a data structure in an unspecified order. Keys are guaranteed to be seen only once per iteration if the data structure is not mutated during iteration. If key is nil, next returns the first key. If next returns nil, there are no more keys to iterate through."
  [ds &opt key])

(defn nil?
  {:params [:any] :ret :boolean :narrows :nil}
  "(nil? x)\n\nCheck if x is nil."
  [x])

(defn not
  {:params [:any] :ret :boolean}
  "(not x)\n\nReturns the boolean inverse of x."
  [x])

(defn not=
  {:params [:any] :ret :boolean}
  "(not= & xs)\n\nCheck if any values in xs are not equal. Returns a boolean."
  [& xs])

(defn number?
  {:params [:any] :ret :boolean :narrows :number}
  "(number? x)\n\nCheck if x is a number."
  [x])

(defn odd?
  {:params [:number] :ret :boolean :narrows :any}
  "(odd? x)\n\nCheck if x is odd."
  [x])

(defn one?
  {:params [:number] :ret :boolean :narrows :any}
  "(one? x)\n\nCheck if x is equal to 1."
  [x])

(defmacro or
  {:params [:any] :ret :any}
  "(or & forms)\n\nEvaluates to the last argument if all preceding elements are falsey, otherwise\nevaluates to the first truthy element."
  [& forms])

(defn os/arch
  {:params [] :ret :keyword}
  "(os/arch)\n\nCheck the ISA that janet was compiled for. Returns one of:\n\n* :x86\n\n* :x64\n\n* :arm\n\n* :aarch64\n\n* :riscv32\n\n* :riscv64\n\n* :sparc\n\n* :wasm\n\n* :s390\n\n* :s390x\n\n* :unknown\n"
  [])

(defn os/cd
  {:params [:string] :ret :nil}
  "(os/cd path)\n\nChange current directory to path. Returns nil on success, errors on failure."
  [path])

(defn os/chmod
  {:params [:string (or :number :string)] :ret :nil}
  "(os/chmod path mode)\n\nChange file permissions, where `mode` is a permission string as returned by `os/perm-string`, or an integer as returned by `os/perm-int`. When `mode` is an integer, it is interpreted as a Unix permission value, best specified in octal, like 8r666 or 8r400. Windows will not differentiate between user, group, and other permissions, and thus will combine all of these permissions. Returns nil.Unsupported on plan9."
  [path mode])

(defn os/clock
  {:params [:keyword? :keyword?] :ret (or :number :tuple)}
  "(os/clock &opt source format)\n\nReturn the current time of the requested clock source.\n\nThe `source` argument selects the clock source to use, when not specified the default is `:realtime`:\n- :realtime: Return the real (i.e., wall-clock) time. This clock is affected by discontinuous   jumps in the system time\n- :monotonic: Return the number of whole + fractional seconds since some fixed point in   time. The clock is guaranteed to be non-decreasing in real time.\n- :cputime: Return the CPU time consumed by this process  (i.e. all threads in the process)\nThe `format` argument selects the type of output, when not specified the default is `:double`:\n- :double: Return the number of seconds + fractional seconds as a double\n- :int: Return the number of seconds as an integer\n- :tuple: Return a 2 integer tuple [seconds, nanoseconds]\n"
  [&opt source format])

(defn os/compiler
  {:params [] :ret :keyword}
  "(os/compiler)\n\nGet the compiler used to compile the interpreter. Returns one of:\n\n* :gcc\n\n* :clang\n\n* :msvc\n\n* :kencc\n\n* :unknown\n\n"
  [])

(defn os/cpu-count
  {:params [:number?] :ret :number?}
  "(os/cpu-count &opt dflt)\n\nGet an approximate number of CPUs available on for this process to use. If unable to get an approximation, will return a default value dflt."
  [&opt dflt])

(defn os/cryptorand
  {:params [:number :buffer] :ret :buffer}
  "(os/cryptorand n &opt buf)\n\nGet or append `n` bytes of good quality random data provided by the OS. Returns a new buffer or `buf`."
  [n &opt buf])

(defn os/cwd
  {:params [] :ret :string}
  "(os/cwd)\n\nReturns the current working directory."
  [])

(defn os/date
  {:params [:number? :boolean?] :ret :struct}
  "(os/date &opt time local)\n\nReturns the given time as a date struct, or the current time if `time` is not given. Date is given in UTC unless `local` is truthy, in which case the date is formatted for the local timezone. Returns a struct with following key values. Note that all numbers are 0-indexed.\n\n* :seconds - number of seconds [0-61]\n\n* :minutes - number of minutes [0-59]\n\n* :hours - number of hours [0-23]\n\n* :month-day - day of month [0-30]\n\n* :month - month of year [0, 11]\n\n* :year - years since year 0 (e.g. 2019)\n\n* :week-day - day of the week [0-6]\n\n* :year-day - day of the year [0-365]\n\n* :dst - if Day Light Savings is in effect\n\nYou can set local timezone by setting TZ environment variable. See tzset(<time.h>) or _tzset(<time.h>) for further details."
  [&opt time local])

(defn os/dir
  {:params [:string :array] :ret :array}
  "(os/dir dir &opt array)\n\nIterate over files and subdirectories in a directory. Returns an array of paths parts, with only the file name or directory name and no prefix."
  [dir &opt array])

(defn os/environ
  {:params [] :ret :table}
  "(os/environ)\n\nGet a copy of the OS environment table."
  [])

(defn os/execute
  {:params [(or [:string] @[:string]) :keyword? (or :struct :table :nil)] :ret :number}
  "(os/execute args &opt flags env)\n\nExecute a program on the system and return the exit code. `args` is an array/tuple of strings. The first string is the name of the program and the remainder are arguments passed to the program. `flags` is a keyword made from the following characters that modifies how the program executes:\n* :e - enables passing an environment to the program. Without 'e', the current environment is inherited.\n* :p - allows searching the current PATH for the program to execute. Without this flag, the first element of `args` must be an absolute path.\n* :x - raises error if exit code is non-zero.\n* :d - prevents the garbage collector terminating the program (if still running) and calling the equivalent of `os/proc-wait` (allows zombie processes).\n`env` is a table/struct mapping environment variables to values. It can also contain the keys :in, :out, and :err, which allow redirecting stdio in the subprocess. :in, :out, and :err should be core/file or core/stream values. If core/stream values are used, the caller is responsible for ensuring pipes do not cause the program to block and deadlock."
  [args &opt flags env])

(defn os/exit
  {:params [:any :boolean?] :ret :nil}
  "(os/exit &opt x force)\n\nExit from janet with an exit code equal to x. If x is not an integer, the exit with status equal the hash of x. If `force` is truthy will exit immediately and skip cleanup code."
  [&opt x force])

(defn os/getenv
  {:params [:string :string?] :ret :string?}
  "(os/getenv variable &opt dflt)\n\nGet the string value of an environment variable."
  [variable &opt dflt])

(defn os/getpid
  {:params [] :ret :number}
  "(os/getpid)\n\nGet the process ID of the current process."
  [])

(defn os/isatty
  {:params [:abstract] :ret :boolean}
  "(os/isatty &opt file)\n\nReturns true if `file` is a terminal. If `file` is not specified, it will default to standard output."
  [&opt file])

(defn os/link
  {:params [:string :string :boolean?] :ret :nil}
  "(os/link oldpath newpath &opt symlink)\n\nCreate a link at newpath that points to oldpath and returns nil. Iff symlink is truthy, creates a symlink. Iff symlink is falsey or not provided, creates a hard link. Does not work on Windows or Plan 9."
  [oldpath newpath &opt symlink])

(defn os/lstat
  {:params [:string (or :table :keyword :nil) :any] :ret :any} # TODO: the C source does not say what it returns
  "(os/lstat path &opt tab|key)\n\nLike os/stat, but don't follow symlinks.\n"
  [path &opt tab|key])

(defn os/mkdir
  {:params [:string] :ret :boolean}
  "(os/mkdir path)\n\nCreate a new directory. The path will be relative to the current directory if relative, otherwise it will be an absolute path. Returns true if the directory was created, false if the directory already exists, and errors otherwise."
  [path])

(defn os/mktime
  {:params [:struct :boolean?] :ret :number}
  "(os/mktime date-struct &opt local)\n\nGet the broken down date-struct time expressed as the number of seconds since January 1, 1970, the Unix epoch. Returns a real number. Date is given in UTC unless `local` is truthy, in which case the date is computed for the local timezone.\n\nInverse function to os/date."
  [date-struct &opt local])

(defn os/open
  {:params [:string :keyword? (or :number :string :nil)] :ret :abstract}
  "(os/open path &opt flags mode)\n\nCreate a stream from a file, like the POSIX open system call. Returns a new stream. `mode` should be a file mode as passed to `os/chmod`, but only if the create flag is given. The default mode is 8r666. Allowed flags are as follows:\n\n  * :r - open this file for reading\n  * :w - open this file for writing\n  * :c - create a new file (O\\_CREATE)\n  * :e - fail if the file exists (O\\_EXCL)\n  * :t - shorten an existing file to length 0 (O\\_TRUNC)\n\n  * :a - append to a file (O\\_APPEND on posix, FILE_APPEND_DATA on windows)\nPosix-only flags:\n\n  * :x - O\\_SYNC\n  * :C - O\\_NOCTTY\n\n  * :N - Turn off O\\_NONBLOCK and disable ev reading/writing\n\nWindows-only flags:\n\n  * :R - share reads (FILE\\_SHARE\\_READ)\n  * :W - share writes (FILE\\_SHARE\\_WRITE)\n  * :D - share deletes (FILE\\_SHARE\\_DELETE)\n  * :H - FILE\\_ATTRIBUTE\\_HIDDEN\n  * :O - FILE\\_ATTRIBUTE\\_READONLY\n  * :F - FILE\\_ATTRIBUTE\\_OFFLINE\n  * :T - FILE\\_ATTRIBUTE\\_TEMPORARY\n  * :d - FILE\\_FLAG\\_DELETE\\_ON\\_CLOSE\n  * :V - Turn off FILE\\_FLAG\\_OVERLAPPED and disable ev reading/writing\n  * :I - set bInheritHandle on the created file so it can be passed to other processes.\n  * :b - FILE\\_FLAG\\_NO\\_BUFFERING\n"
  [path &opt flags mode])

(defn os/perm-int
  {:params [(or :string :buffer)] :ret :number}
  "(os/perm-int bytes)\n\nParse a 9-character permission string and return an integer that can be used by chmod."
  [bytes])

(defn os/perm-string
  {:params [:number] :ret :string}
  "(os/perm-string int)\n\nConvert a Unix octal permission value from a permission integer as returned by `os/stat` to a human readable string, that follows the formatting of Unix tools like `ls`. Returns the string as a 9-character string of r, w, x and - characters. Does not include the file/directory/symlink character as rendered by `ls`."
  [int])

(defn os/pipe
  {:params [:keyword] :ret :tuple}
  "(os/pipe &opt flags)\n\nCreate a readable stream and a writable stream that are connected. Returns a two-element tuple where the first element is a readable stream and the second element is the writable stream. `flags` is a keyword set of flags to disable non-blocking settings on the ends of the pipe. This may be desired if passing the pipe to a subprocess with `os/spawn`.\n\n* :W - sets the writable end of the pipe to a blocking stream.\n* :R - sets the readable end of the pipe to a blocking stream.\n\nBy default, both ends of the pipe are non-blocking for use with the `ev` module."
  [&opt flags])

(defn os/posix-chroot
  {:params [:string] :ret :nil}
  "(os/posix-chroot dirname)\n\nCall `chroot` to change the root directory to `dirname`. Not supported on all systems (POSIX only)."
  [dirname])

(defn os/posix-exec
  {:params [(or [:string] @[:string]) :keyword? (or :struct :table :nil)] :ret :nil}
  "(os/posix-exec args &opt flags env)\n\nUse the execvpe or execve system calls to replace the current process with an interface similar to os/execute. However, instead of creating a subprocess, the current process is replaced. Is not supported on Windows, and does not allow redirection of stdio."
  [args &opt flags env])

(defn os/posix-fork
  {:params [] :ret :abstract?}
  "(os/posix-fork)\n\nMake a `fork` system call and create a new process. Return nil if in the new process, otherwise a core/process object (as returned by os/spawn). Not supported on all systems (POSIX and Plan 9 only)."
  [])

(defn os/proc-close
  {:params [:abstract] :ret :number?}
  "(os/proc-close proc)\n\nClose pipes created for subprocess `proc` by `os/spawn` if they have not been closed. Then, if `proc` is not being waited for, wait. If this function waits, when `proc` completes, return the exit code of `proc`. Otherwise, return nil."
  [proc])

(defn os/proc-kill
  {:params [:abstract :boolean? (or :keyword :number :nil)] :ret :any} # TODO: the C source does not say what it returns
  "(os/proc-kill proc &opt wait signal)\n\nKill the subprocess `proc` by sending SIGKILL to it on POSIX systems, or by closing the process handle on Windows. If `proc` has already completed, raise an error. If `wait` is truthy, will wait for `proc` to complete and return the exit code (this will raise an error if `proc` is being waited for). Otherwise, return `proc`. If `signal` is provided, send it instead of SIGKILL. Signal keywords are named after their C counterparts but in lowercase with the leading SIG stripped. `signal` is ignored on Windows."
  [proc &opt wait signal])

(defn os/proc-wait
  {:params [:abstract] :ret :number}
  "(os/proc-wait proc)\n\nSuspend the current fiber until the subprocess `proc` completes. Once `proc` completes, return the exit code of `proc`. If called more than once on the same core/process value, will raise an error. When creating subprocesses using `os/spawn`, this function should be called on the returned value to avoid zombie processes."
  [proc])

(defn os/readlink
  {:params [:string] :ret :string}
  "(os/readlink path)\n\nRead the contents of a symbolic link. Does not work on Windows.\n"
  [path])

(defn os/realpath
  {:params [:string] :ret :string}
  "(os/realpath path)\n\nGet the absolute path for a given path, following ../, ./, and symlinks. Returns an absolute path as a string."
  [path])

(defn os/rename
  {:params [:string :string] :ret :nil}
  "(os/rename oldname newname)\n\nRename a file on disk to a new path. Returns nil."
  [oldname newname])

(defn os/rm
  {:params [:string] :ret :nil}
  "(os/rm path)\n\nDelete a file. Returns nil."
  [path])

(defn os/rmdir
  {:params [:string] :ret :nil}
  "(os/rmdir path)\n\nDelete a directory. The directory must be empty to succeed."
  [path])

(defn os/setenv
  {:params [:string :string?] :ret :nil}
  "(os/setenv variable value)\n\nSet an environment variable."
  [variable value])

(defn os/setlocale
  {:params [:string? :keyword?] :ret :string?}
  "(os/setlocale &opt locale category)\n\nSet the system locale, which affects how dates and numbers are formatted. Passing nil to locale will return the current locale. Category can be one of:\n\n * :all (default)\n * :collate\n * :ctype\n * :monetary\n * :numeric\n * :time\n\nReturns the new locale if set successfully, otherwise nil. Note that this will affect other functions such as `os/strftime` and even `printf`."
  [&opt locale category])

(defn os/shell
  {:params [:string] :ret :number}
  "(os/shell str)\n\nPass a command string str directly to the system shell."
  [str])

(defn os/sigaction
  {:params [:keyword :function? :boolean?] :ret :nil}
  "(os/sigaction which &opt handler interrupt-interpreter)\n\nAdd a signal handler for a given action. Use nil for the `handler` argument to remove a signal handler. All signal handlers are the same as supported by `os/proc-kill`."
  [which &opt handler interrupt-interpreter])

(defn os/sleep
  {:params [:number] :ret :nil}
  "(os/sleep n)\n\nSuspend the program for `n` seconds. `n` can be a real number. Returns nil."
  [n])

(defn os/spawn
  {:params [(or [:string] @[:string]) :keyword? (or :struct :table :nil)] :ret :abstract}
  "(os/spawn args &opt flags env)\n\nExecute a program on the system and return a core/process value representing the spawned subprocess. Takes the same arguments as `os/execute` but does not wait for the subprocess to complete. Unlike `os/execute`, the value `:pipe` can be used for :in, :out and :err keys in `env`. If used, the returned core/process will have a writable stream in the :in field and readable streams in the :out and :err fields. On non-Windows systems, the subprocess PID will be in the :pid field. The caller is responsible for waiting on the process (e.g. by calling `os/proc-wait` on the returned core/process value) to avoid creating zombie process. After the subprocess completes, the exit value is in the :return-code field. If `flags` includes 'x', a non-zero exit code will cause a waiting fiber to raise an error. The use of `:pipe` may fail if there are too many active file descriptors. The caller is responsible for closing pipes created by `:pipe` (either individually or using `os/proc-close`). Similar to `os/execute`, the caller is responsible for ensuring pipes do not cause the program to block and deadlock. As a special case, the stream passed to `:err` can be the keyword `:out` to redirect stderr to stdout in the subprocess."
  [args &opt flags env])

(defn os/stat
  {:params [:string (or :table :keyword :nil) :any] :ret :any} # TODO: the C source does not say what it returns
  "(os/stat path &opt tab|key)\n\nGets information about a file or directory. Returns a table if the second argument is a keyword, returns only that information from stat. If the file or directory does not exist, returns nil. The keys are:\n\n* :dev - the device that the file is on\n\n* :mode - the type of file, one of :file, :directory, :block, :character, :fifo, :socket, :link, or :other\n\n* :int-permissions - A Unix permission integer like 8r744\n\n* :permissions - A Unix permission string like \"rwxr--r--\"\n\n* :uid - File uid\n\n* :gid - File gid\n\n* :nlink - number of links to file\n\n* :rdev - Real device of file. 0 on Windows\n\n* :size - size of file in bytes\n\n* :blocks - number of blocks in file. 0 on Windows\n\n* :blocksize - size of blocks in file. 0 on Windows\n\n* :accessed - timestamp when file last accessed\n\n* :changed - timestamp when file last changed (permissions changed)\n\n* :modified - timestamp when file last modified (content changed)\n"
  [path &opt tab|key])

(defn os/strftime
  {:params [:string :number? :boolean?] :ret :string}
  "(os/strftime fmt &opt time local)\n\nFormat the given time as a string, or the current time if `time` is not given. The time is formatted according to the same rules as the ISO C89 function strftime(). The time is formatted in UTC unless `local` is truthy, in which case the date is formatted for the local timezone. You can set local timezone by setting TZ environment variable. See tzset(<time.h>) or _tzset(<time.h>) for further details."
  [fmt &opt time local])

(defn os/symlink
  {:params [:string :string] :ret :nil}
  "(os/symlink oldpath newpath)\n\nCreate a symlink from oldpath to newpath, returning nil. Same as `(os/link oldpath newpath true)`."
  [oldpath newpath])

(defn os/time
  {:params [] :ret :number}
  "(os/time)\n\nGet the current time expressed as the number of whole seconds since January 1, 1970, the Unix epoch. Returns a real number."
  [])

(defn os/touch
  {:params [:string :number :number] :ret :nil}
  "(os/touch path &opt actime modtime)\n\nUpdate the access time and modification times for a file. By default, sets times to the current time."
  [path &opt actime modtime])

(defn os/umask
  {:params [:number] :ret :number}
  "(os/umask mask)\n\nSet a new umask, returns the old umask."
  [mask])

(defn os/which
  {:params [:keyword?] :ret (or :keyword :boolean)}
  "(os/which &opt test)\n\nCheck the current operating system. If `test` is nil or unset, Returns one of:\n\n* :windows\n\n* :mingw\n\n* :cygwin\n\n* :macos\n\n* :web - Web assembly (emscripten)\n\n* :linux\n\n* :freebsd\n\n* :openbsd\n\n* :netbsd\n\n* :dragonfly\n\n* :bsd\n\n* :posix - A POSIX compatible system (default)\n\nMay also return a custom keyword specified at build time. Is `test` is truthy, will check if the current operating system equals `test` and return true if they are the same, false otherwise."
  [&opt test])

(defn pairs
  {:params [:any] :ret @[[:any :any]]}
  "(pairs x)\n\nGet the key-value pairs of an associative data structure."
  [x])

(defn parse
  {:params [(or :string :buffer)] :ret :any}
  "(parse str)\n\nParse a string and return the first value. For complex parsing, such as for a repl with error handling,\nuse the parser api."
  [str])

(defn parse-all
  {:params [(or :string :buffer)] :ret @[:any]}
  "(parse-all str)\n\nParse a string and return all parsed values. For complex parsing, such as for a repl with error handling,\nuse the parser api."
  [str])

(defn parser/byte
  {:params [:abstract :number] :ret :abstract}
  "(parser/byte parser b)\n\nInput a single byte `b` into the parser byte stream. Returns the parser."
  [parser b])

(defn parser/clone
  {:params [:abstract] :ret :abstract}
  "(parser/clone p)\n\nCreates a deep clone of a parser that is identical to the input parser. This cloned parser can be used to continue parsing from a good checkpoint if parsing later fails. Returns a new parser."
  [p])

(defn parser/consume
  {:params [:abstract (or :string :buffer :symbol :keyword) :number] :ret :number}
  "(parser/consume parser bytes &opt index)\n\nInput bytes into the parser and parse them. Will not throw errors if there is a parse error. Starts at the byte index given by `index`. Returns the number of bytes read."
  [parser bytes &opt index])

(defn parser/eof
  {:params [:abstract] :ret :abstract}
  "(parser/eof parser)\n\nIndicate to the parser that the end of file was reached. This puts the parser in the :dead state."
  [parser])

(defn parser/error
  {:params [:abstract] :ret :string?}
  "(parser/error parser)\n\nIf the parser is in the error state, returns the message associated with that error. Otherwise, returns nil. Also flushes the parser state and parser queue, so be sure to handle everything in the queue before calling `parser/error`."
  [parser])

(defn parser/flush
  {:params [:abstract] :ret :abstract}
  "(parser/flush parser)\n\nClears the parser state and parse queue. Can be used to reset the parser if an error was encountered. Does not reset the line and column counter, so to begin parsing in a new context, create a new parser."
  [parser])

(defn parser/has-more
  {:params [:abstract] :ret :boolean}
  "(parser/has-more parser)\n\nCheck if the parser has more values in the value queue."
  [parser])

(defn parser/insert
  {:params [:abstract :any] :ret :abstract}
  "(parser/insert parser value)\n\nInsert a value into the parser. This means that the parser state can be manipulated in between chunks of bytes. This would allow a user to add extra elements to arrays and tuples, for example. Returns the parser."
  [parser value])

(defn parser/new
  {:params [] :ret :abstract}
  "(parser/new)\n\nCreates and returns a new parser object. Parsers are state machines that can receive bytes and generate a stream of values."
  [])

(defn parser/produce
  {:params [:abstract :boolean?] :ret :any} # TODO: the C source does not say what it returns
  "(parser/produce parser &opt wrap)\n\nDequeue the next value in the parse queue. Will return nil if no parsed values are in the queue, otherwise will dequeue the next value. If `wrap` is truthy, will return a 1-element tuple that wraps the result. This tuple can be used for source-mapping purposes."
  [parser &opt wrap])

(defn parser/state
  {:params [:abstract :keyword?] :ret :any} # TODO: the C source does not say what it returns
  "(parser/state parser &opt key)\n\nReturns a representation of the internal state of the parser. If a key is passed, only that information about the state is returned. Allowed keys are:\n\n* :delimiters - Each byte in the string represents a nested data structure. For example, if the parser state is '([\"', then the parser is in the middle of parsing a string inside of square brackets inside parentheses. Can be used to augment a REPL prompt.\n\n* :frames - Each table in the array represents a 'frame' in the parser state. Frames contain information about the start of the expression being parsed as well as the type of that expression and some type-specific information."
  [parser &opt key])

(defn parser/status
  {:params [:abstract] :ret :keyword}
  "(parser/status parser)\n\nGets the current status of the parser state machine. The status will be one of:\n\n* :pending - a value is being parsed.\n\n* :error - a parsing error was encountered.\n\n* :root - the parser can either read more values or safely terminate."
  [parser])

(defn parser/where
  {:params [:abstract :number :number] :ret :tuple}
  "(parser/where parser &opt line col)\n\nReturns the current line number and column of the parser's internal state. If line is provided, the current line number of the parser is first set to that value. If column is also provided, the current column number of the parser is also first set to that value."
  [parser &opt line col])

(defn partial
  {:params [:function :any] :ret :function}
  "(partial f & more)\n\nPartial function application."
  [f & more])

(defn partition
  {:params [:number (or [a] @[a])] :ret @[[a]]}
  "(partition n ind)\n\nPartition an indexed data structure `ind` into tuples\nof size `n`. Returns a new array."
  [n ind])

(defn partition-by
  {:params [(fn [a] :any) (or [a] @[a])] :ret @[[a]]}
  "(partition-by f ind)\n\nPartition elements of a sequential data structure by a representative function `f`. Partitions\nsplit when `(f x)` changes values when iterating to the next element `x` of `ind`. Returns a new array\nof arrays."
  [f ind])

(defn peg/compile
  {:params [Pattern] :ret :abstract}
  "(peg/compile peg)\n\nCompiles a peg source data structure into a <core/peg>. This will speed up matching if the same peg will be used multiple times. `(dyn :peg-grammar)` replaces `default-peg-grammar` for the grammar of the peg."
  [peg])

(defn peg/find
  {:params [Pattern (or :string :buffer) :number? :any] :ret :number?}
  "(peg/find peg text &opt start & args)\n\nFind first index where the peg matches in text. Returns an integer, or nil if not found."
  [peg text &opt start & args])

(defn peg/find-all
  {:params [Pattern (or :string :buffer) :number? :any] :ret @[:number]}
  "(peg/find-all peg text &opt start & args)\n\nFind all indexes where the peg matches in text. Returns an array of integers."
  [peg text &opt start & args])

(defn peg/match
  {:params [Pattern (or :string :buffer) :number? :any] :ret (or @[:any] :nil)}
  "(peg/match peg text &opt start & args)\n\nMatch a Parsing Expression Grammar to a byte string and return an array of captured values. Returns nil if text does not match the language defined by peg. The syntax of PEGs is documented on the Janet website."
  [peg text &opt start & args])

(defn peg/replace
  {:params [Pattern :any (or :string :buffer) :number? :any] :ret :buffer}
  "(peg/replace peg subst text &opt start & args)\n\nReplace first match of `peg` in `text` with `subst`, returning a new buffer. The peg does not need to make captures to do replacement. If `subst` is a function, it will be called with the matching text followed by any captures. If no matches are found, returns the input string in a new buffer."
  [peg subst text &opt start & args])

(defn peg/replace-all
  {:params [Pattern :any (or :string :buffer) :number? :any] :ret :buffer}
  "(peg/replace-all peg subst text &opt start & args)\n\nReplace all matches of `peg` in `text` with `subst`, returning a new buffer. The peg does not need to make captures to do replacement. If `subst` is a function, it will be called with the matching text followed by any captures."
  [peg subst text &opt start & args])

(defn pos?
  {:params [:number] :ret :boolean :narrows :any}
  "(pos? x)\n\nCheck if x is greater than 0."
  [x])

(defn postwalk
  {:params [(fn [:any] :any) :any] :ret :any}
  "(postwalk f form)\n\nDo a post-order traversal of a data structure and call `(f x)`\non every visitation."
  [f form])

(defn pp
  {:params [:any] :ret :nil}
  "(pp x)\n\nPretty-print to stdout or `(dyn *out*)`. The format string used is `(dyn *pretty-format* \"%q\")`."
  [x])

(defn prewalk
  {:params [(fn [:any] :any) :any] :ret :any}
  "(prewalk f form)\n\nSimilar to `postwalk`, but do pre-order traversal."
  [f form])

(defn prin
  {:params [:any] :ret :nil}
  "(prin & xs)\n\nSame as `print`, but does not add trailing newline."
  [& xs])

(defn prinf
  {:params [:string :any] :ret :nil}
  "(prinf fmt & xs)\n\nLike `printf` but with no trailing newline."
  [fmt & xs])

(defn print
  {:params [:any] :ret :nil}
  "(print & xs)\n\nPrint values to the console (standard out). Value are converted to strings if they are not already. After printing all values, a newline character is printed. Use the value of `(dyn :out stdout)` to determine what to push characters to. Expects `(dyn :out stdout)` to be either a core/file or a buffer. Returns nil."
  [& xs])

(defn printf
  {:params [:string :any] :ret :nil}
  "(printf fmt & xs)\n\nPrints output formatted as if with `(string/format fmt ;xs)` to `(dyn :out stdout)` with a trailing newline."
  [fmt & xs])

(defn product
  {:params [(or [:number] @[:number])] :ret :number}
  "(product xs)\n\nReturns the product of xs. If xs is empty, returns 1."
  [xs])

(defmacro prompt
  {:params [:any :any] :ret :any}
  "(prompt tag & body)\n\nSet up a checkpoint that can be returned to. `tag` should be a value\nthat is used in a `return` statement, like a keyword."
  [tag & body])

(defn propagate
  {:params [:any :fiber] :ret :never}
  "(propagate x fiber)\n\nPropagate a signal from a fiber to the current fiber and set the last value of the current fiber to `x`.  The signal value is then available as the status of the current fiber. The resulting stack trace from the current fiber will include frames from fiber. If fiber is in a state that can be resumed, resuming the current fiber will first resume `fiber`. This function can be used to re-raise an error without losing the original stack trace."
  [x fiber])

(defmacro protect
  {:params [:any] :ret [:boolean :any]}
  "(protect & body)\n\nEvaluate expressions, while capturing any errors. Evaluates to a tuple\nof two elements. The first element is true if successful, false if an\nerror, and the second is the return value or error."
  [& body])

(defn put
  {:params [a :any :any] :ret a}
  "(put ds key value)\n\nAssociate a key with a value in any mutable associative data structure. Indexed data structures (arrays and buffers) only accept non-negative integer keys, and will expand if an out of bounds value is provided. In an array, extra space will be filled with nils, and in a buffer, extra space will be filled with 0 bytes. In a table, putting a key that is contained in the table prototype will hide the association defined by the prototype, but will not mutate the prototype table. Putting a value nil into a table will remove the key from the table. Returns the data structure ds."
  [ds key value])

(defn put-in
  {:params [a (or [:any] @[:any]) :any] :ret a}
  "(put-in ds ks v)\n\nPut a value into a nested data structure `ds`. Looks into `ds` via\na sequence of keys. Missing data structures will be replaced with tables. Returns\nthe modified, original data structure."
  [ds ks v])

(defn quit
  {:params [:any] :ret :never}
  "(quit &opt value)\n\nTries to exit from the current repl or run-context. Does not always exit the application.\nWorks by setting the :exit dynamic binding to true. Passing a non-nil `value` here will cause the outer\nrun-context to return that value."
  [&opt value])

(defn range
  {:params [:number] :ret @[:number]}
  "(range & args)\n\nCreate an array of values [start, end) with a given step. With one argument, returns a range [0, end). With two arguments, returns a range [start, end). With three, returns a range with optional step size."
  [& args])

(defn reduce
  {:params [(fn [b a] b) b (or [a] @[a])] :ret b}
  "(reduce f init ind)\n\nReduce, also know as fold-left in many languages, transforms\nan indexed type (array, tuple) with a function to produce a value by applying `f` to\neach element in order. `f` is a function of 2 arguments, `(f accum el)`, where\n`accum` is the initial value and `el` is the next value in the indexed type `ind`.\n`f` returns a value that will be used as `accum` in the next call to `f`. `reduce`\nreturns the value of the final call to `f`."
  [f init ind])

(defn reduce2
  {:params [(fn [a a] a) (or [a] @[a])] :ret a?}
  "(reduce2 f ind)\n\nThe 2-argument version of `reduce` that does not take an initialization value.\nInstead, the first element of the array is used for initialization. If `ind` is empty, will evaluate to nil."
  [f ind])

(defmacro repeat
  {:params [:number :any] :ret :nil}
  "(repeat n & body)\n\nEvaluate body n times. If n is negative, body will be evaluated 0 times. Evaluates to nil."
  [n & body])

(defn repl
  {:params [:function? :function? (or :struct :table :nil) :abstract? :function?] :ret :any}
  "(repl &opt chunks onsignal env parser read)\n\nRun a repl. The first parameter is an optional function to call to\nget a chunk of source code that should return nil for end of file.\nThe second parameter is a function that is called when a signal is\ncaught. One can provide an optional environment table to run\nthe repl in, as well as an optional parser or read function to pass\nto `run-context`."
  [&opt chunks onsignal env parser read])

(defn require
  {:params [:string :any] :ret :table}
  "(require path & args)\n\nRequire a module with the given name. Will search all of the paths in\n`module/paths`. Returns the new environment\nreturned from compiling and running the file."
  [path & args])

(defn resume
  {:params [:fiber :any] :ret :any}
  "(resume fiber &opt x)\n\nResume a new or suspended fiber and optionally pass in a value to the fiber that will be returned to the last yield in the case of a pending fiber, or the argument to the dispatch function in the case of a new fiber. Returns either the return result of the fiber's dispatch function, or the value from the next yield call in fiber."
  [fiber &opt x])

(defn return
  {:params [:fiber :any] :ret :never}
  "(return to &opt value)\n\nReturn to a prompt point."
  [to &opt value])

(defn reverse
  {:params [(or [a] @[a] :string :buffer)] :ret (or @[a] :buffer)}
  "(reverse t)\n\nReverses the order of the elements in a given array or tuple and returns\na new array. If a string or buffer is provided, returns a buffer instead."
  [t])

(defn reverse!
  {:params [(or @[a] :buffer)] :ret (or @[a] :buffer)}
  "(reverse! t)\n\nReverses the order of the elements in a given array or buffer and returns it\nmutated."
  [t])

(def root-env
  {:type :table}
  "The root environment used to create environments with (make-env)."
  nil)

(defn run-context
  {:params [(or :struct :table)] :ret :any}
  "(run-context opts)\n\nRun a context. This evaluates expressions in an environment,\nand encapsulates the parsing, compilation, and evaluation.\nReturns `(in environment :exit-value environment)` when complete.\n`opts` is a table or struct of options. The options are as follows:\n\n  * `:chunks` -- callback to read into a buffer - default is getline\n\n  * `:on-parse-error` -- callback when parsing fails - default is bad-parse\n\n  * `:env` -- the environment to compile against - default is the current env\n\n  * `:source` -- source path for better errors (use keywords for non-paths) - default\n    is `:<anonymous>`\n\n  * `:on-compile-error` -- callback when compilation fails - default is bad-compile\n\n  * `:on-compile-warning` -- callback for any linting error - default is warn-compile\n\n  * `:evaluator` -- callback that executes thunks. Signature is (evaluator thunk source\n    env where)\n\n  * `:on-status` -- callback when a value is evaluated - default is debug/stacktrace.\n\n  * `:fiber-flags` -- what flags to wrap the compilation fiber with. Default is :ia.\n\n  * `:expander` -- an optional function that is called on each top level form before\n    being compiled.\n\n  * `:parser` -- provide a custom parser that implements the same interface as Janet's\n    built-in parser.\n\n  * `:read` -- optional function to get the next form, called like `(read env source)`.\n    Overrides all parsing."
  [opts])

(defn sandbox
  {:params [:keyword] :ret :nil}
  "(sandbox & forbidden-capabilities)\n\nDisable feature sets to prevent the interpreter from using certain system resources. Once a feature is disabled, there is no way to re-enable it. Capabilities can be:\n\n* :all - disallow all (except IO to stdout, stderr, and stdin)\n* :asm - disallow calling `asm` and `disasm` functions.\n* :chroot - disallow calling `os/posix-chroot`\n* :compile - disallow calling `compile`. This will disable a lot of functionality, such as `eval`.\n* :env - disallow reading and write env variables\n* :ffi - disallow FFI (recommended if disabling anything else)\n* :ffi-define - disallow loading new FFI modules and binding new functions\n* :ffi-jit - disallow calling `ffi/jitfn`\n* :ffi-use - disallow using any previously bound FFI functions and memory-unsafe functions.\n* :fs - disallow access to the file system\n* :fs-read - disallow read access to the file system\n* :fs-temp - disallow creating temporary files\n* :fs-write - disallow write access to the file system\n* :hrtime - disallow high-resolution timers\n* :modules - disallow load dynamic modules (natives)\n* :net - disallow network access\n* :net-connect - disallow making outbound network connections\n* :net-listen - disallow accepting inbound network connections\n* :sandbox - disallow calling this function\n* :signal - disallow adding or removing signal handlers\n* :subprocess - disallow running subprocesses\n* :threads - disallow spawning threads with `ev/thread`. Certain helper threads may still be spawned.\n* :unmarshal - disallow calling the unmarshal function.\n"
  [& forbidden-capabilities])

(defn scan-number
  {:params [(or :string :buffer :symbol :keyword) :number?] :ret :number?}
  "(scan-number str &opt base)\n\nParse a number from a byte sequence and return that number, either an integer or a real. The number must be in the same format as numbers in janet source code. Will return nil on an invalid number. Optionally provide a base - if a base is provided, no radix specifier is expected at the beginning of the number."
  [str &opt base])

(defmacro seq
  {:params [:tuple :any] :ret :array}
  "(seq head & body)\n\nSimilar to `loop`, but accumulates the loop body into an array and returns that.\nSee `loop` for details."
  [head & body])

(defn setdyn
  {:params [:keyword a] :ret a}
  "(setdyn key value)\n\nSet a dynamic binding. Returns value."
  [key value])

(defmacro short-fn
  {:params [:any :symbol?] :ret :function}
  "(short-fn arg &opt name)\n\nShorthand for `fn`. Arguments are given as `$n`, where `n` is the\n0-indexed argument of the function. `$` is also an alias for the\nfirst (index 0) argument. The `$&` symbol will make the anonymous\nfunction variadic if it appears in the body of the function, and\ncan be combined with positional arguments."
  [arg &opt name])

(defn signal
  {:params [(or :keyword :number) :any] :ret :any} # TODO: the C source does not say what it returns
  "(signal what x)\n\nRaise a signal with payload x. `what` can be an integer\nfrom 0 through 7 indicating user(0-7), or one of:\n\n* :ok\n* :error\n* :debug\n* :yield\n* :user(0-7)\n* :interrupt\n* :await"
  [what x])

(defn slice
  {:params [(or [a] @[a] :string :buffer) :number? :number?] :ret (or [a] :string)}
  "(slice x &opt start end)\n\nExtract a sub-range of an indexed data structure or byte sequence."
  [x &opt start end])

(defn slurp
  {:params [:string] :ret :buffer}
  "(slurp path)\n\nRead all data from a file with name `path` and then close the file."
  [path])

(defn some
  {:params [(fn [a] b) (or [a] @[a]) (or [:any] @[:any])] :ret b?}
  "(some pred ind & inds)\n\nReturns nil if applying `pred` to every value in a data\nstructure `ind` results in only falsey values, but only if no\n`inds` are provided. Multiple data structures can be handled\nif each `inds` is a data structure and `pred` is a function\nof arity one more than the number of `inds`. Returns the first\ntruthy result encountered. Note that `pred` is only called as\nmany times as the length of the shortest of `ind` and each of\n`inds`. If `ind` or any of `inds` are empty, returns nil."
  [pred ind & inds])

(defn sort
  {:params [@[a] (fn [a a] :any)] :ret @[a]}
  "(sort ind &opt before?)\n\nSorts `ind` in-place, and returns it. Uses quick-sort and is not a stable sort.\nIf a `before?` comparator function is provided, sorts elements using that,\notherwise uses `<`."
  [ind &opt before?])

(defn sort-by
  {:params [(fn [a] :any) @[a]] :ret @[a]}
  "(sort-by f ind)\n\nSorts `ind` in-place by calling a function `f` on each element and\ncomparing the result with `<`."
  [f ind])

(defn sorted
  {:params [(or [a] @[a]) (fn [a a] :any)] :ret @[a]}
  "(sorted ind &opt before?)\n\nReturns a new sorted array without modifying the old one.\nIf a `before?` comparator function is provided, sorts elements using that,\notherwise uses `<`."
  [ind &opt before?])

(defn sorted-by
  {:params [(fn [a] :any) (or [a] @[a])] :ret @[a]}
  "(sorted-by f ind)\n\nReturns a new sorted array that compares elements by invoking\na function `f` on each element and comparing the result with `<`."
  [f ind])

(defn spit
  {:params [:string (or :string :buffer) :string?] :ret :nil}
  "(spit path contents &opt mode)\n\nWrite `contents` to a file at `path`. Can optionally append to the file."
  [path contents &opt mode])

(def stderr
  {:type :abstract}
  "The standard error file."
  nil)

(def stdin
  {:type :abstract}
  "The standard input file."
  nil)

(def stdout
  {:type :abstract}
  "The standard output file."
  nil)

(defn string
  {:params [:any] :ret :string}
  "(string & xs)\n\nCreates a string by concatenating the elements of `xs` together. If an element is not a byte sequence, it is converted to bytes via `describe`. Returns the new string."
  [& xs])

(defn string/ascii-lower
  {:params [(or :string :buffer :symbol :keyword)] :ret :string}
  "(string/ascii-lower str)\n\nReturns a new string where all bytes are replaced with the lowercase version of themselves in ASCII. Does only a very simple case check, meaning no unicode support."
  [str])

(defn string/ascii-upper
  {:params [(or :string :buffer :symbol :keyword)] :ret :string}
  "(string/ascii-upper str)\n\nReturns a new string where all bytes are replaced with the uppercase version of themselves in ASCII. Does only a very simple case check, meaning no unicode support."
  [str])

(defn string/bytes
  {:params [(or :string :buffer :symbol :keyword)] :ret :tuple}
  "(string/bytes str)\n\nReturns a tuple of integers that are the byte values of the string."
  [str])

(defn string/check-set
  {:params [(or :string :buffer :symbol :keyword) (or :string :buffer :symbol :keyword)] :ret :boolean}
  "(string/check-set set str)\n\nChecks that the string `str` only contains bytes that appear in the string `set`. Returns true if all bytes in `str` appear in `set`, false if some bytes in `str` do not appear in `set`."
  [set str])

(defn string/find
  {:params [(or :string :buffer) (or :string :buffer) :number?] :ret :number?}
  "(string/find patt str &opt start-index)\n\nSearches for the first instance of pattern `patt` in string `str`. Returns the index of the first character in `patt` if found, otherwise returns nil."
  [patt str &opt start-index])

(defn string/find-all
  {:params [(or :string :buffer) (or :string :buffer) :number?] :ret @[:number]}
  "(string/find-all patt str &opt start-index)\n\nSearches for all instances of pattern `patt` in string `str`. Returns an array of all indices of found patterns. Overlapping instances of the pattern are counted individually, meaning a byte in `str` may contribute to multiple found patterns."
  [patt str &opt start-index])

(defn string/format
  {:params [:string :any] :ret :string}
  "(string/format format & values)\n\nSimilar to C's `snprintf`, but specialized for operating with Janet values. Returns a new string.\n\nThe following conversion specifiers are supported, where the upper case specifiers generate upper case output:\n- `c`: ASCII character.\n- `d`, `i`: integer, formatted as a decimal number.\n- `x`, `X`: integer, formatted as a hexadecimal number.\n- `o`: integer, formatted as an octal number.\n- `f`, `F`: floating point number, formatted as a decimal number.\n- `e`, `E`: floating point number, formatted in scientific notation.\n- `g`, `G`: floating point number, formatted in its shortest form.\n- `a`, `A`: floating point number, formatted as a hexadecimal number.\n- `s`: formatted as a string, precision indicates padding and maximum length.\n- `t`: emit the type of the given value.\n- `v`: format with (describe x)\n- `V`: format with (string x)\n- `j`: format to jdn (Janet data notation).\n\nThe following conversion specifiers are used for \"pretty-printing\", where the upper-case variants generate colored output. These specifiers can take a precision argument to specify the maximum nesting depth to print.\n- `p`, `P`: pretty format, truncating if necessary\n- `m`, `M`: pretty format without truncating.\n- `q`, `Q`: pretty format on one line, truncating if necessary.\n- `n`, `N`: pretty format on one line without truncation.\n"
  [format & values])

(defn string/from-bytes
  {:params [:number] :ret :string}
  "(string/from-bytes & byte-vals)\n\nCreates a string from integer parameters with byte values. All integers will be coerced to the range of 1 byte 0-255."
  [& byte-vals])

(defn string/has-prefix?
  {:params [(or :string :buffer :symbol :keyword) (or :string :buffer :symbol :keyword)] :ret :boolean :narrows :any}
  "(string/has-prefix? pfx str)\n\nTests whether `str` starts with `pfx`."
  [pfx str])

(defn string/has-suffix?
  {:params [(or :string :buffer :symbol :keyword) (or :string :buffer :symbol :keyword)] :ret :boolean :narrows :any}
  "(string/has-suffix? sfx str)\n\nTests whether `str` ends with `sfx`."
  [sfx str])

(defn string/join
  {:params [(or :tuple :array) (or :string :buffer :symbol :keyword)] :ret :string}
  "(string/join parts &opt sep)\n\nJoins an array of strings into one string, optionally separated by a separator string `sep`."
  [parts &opt sep])

(defn string/repeat
  {:params [(or :string :buffer :symbol :keyword) :number] :ret :string}
  "(string/repeat bytes n)\n\nReturns a string that is `n` copies of `bytes` concatenated."
  [bytes n])

(defn string/replace
  {:params [(or :string :buffer :abstract) (or :string :buffer :function) (or :string :buffer)] :ret :string}
  "(string/replace patt subst str)\n\nReplace the first occurrence of `patt` with `subst` in the string `str`. If `subst` is a function, it will be called with `patt` only if a match is found, and should return the actual replacement text to use. Will return the new string if `patt` is found, otherwise returns `str`."
  [patt subst str])

(defn string/replace-all
  {:params [(or :string :buffer :abstract) (or :string :buffer :function) (or :string :buffer)] :ret :string}
  "(string/replace-all patt subst str)\n\nReplace all instances of `patt` with `subst` in the string `str`. Overlapping matches will not be counted, only the first match in such a span will be replaced. If `subst` is a function, it will be called with `patt` once for each match, and should return the actual replacement text to use. Will return the new string if `patt` is found, otherwise returns `str`."
  [patt subst str])

(defn string/reverse
  {:params [(or :string :buffer :symbol :keyword)] :ret :string}
  "(string/reverse str)\n\nReturns a string that is the reversed version of `str`."
  [str])

(defn string/slice
  {:params [(or :string :buffer :symbol :keyword) :number? :number?] :ret :string}
  "(string/slice bytes &opt start end)\n\nReturns a substring from a byte sequence. The substring is from index `start` inclusive to index `end`, exclusive. All indexing is from 0. `start` and `end` can also be negative to indicate indexing from the end of the string. Note that if `start` is negative it is exclusive, and if `end` is negative it is inclusive, to allow a full negative slice range."
  [bytes &opt start end])

(defn string/split
  {:params [(or :string :buffer) (or :string :buffer) :number? :number?] :ret @[:string]}
  "(string/split delim str &opt start limit)\n\nSplits a string `str` with delimiter `delim` and returns an array of substrings. The substrings will not contain the delimiter `delim`. If `delim` is not found, the returned array will have one element. Will start searching for `delim` at the index `start` (if provided), and return up to a maximum of `limit` results (if provided)."
  [delim str &opt start limit])

(defn string/trim
  {:params [(or :string :buffer) (or :string :buffer :nil)] :ret :string}
  "(string/trim str &opt set)\n\nTrim leading and trailing whitespace from a byte sequence. If the argument `set` is provided, consider only characters in `set` to be whitespace."
  [str &opt set])

(defn string/triml
  {:params [(or :string :buffer) (or :string :buffer :nil)] :ret :string}
  "(string/triml str &opt set)\n\nTrim leading whitespace from a byte sequence. If the argument `set` is provided, consider only characters in `set` to be whitespace."
  [str &opt set])

(defn string/trimr
  {:params [(or :string :buffer) (or :string :buffer :nil)] :ret :string}
  "(string/trimr str &opt set)\n\nTrim trailing whitespace from a byte sequence. If the argument `set` is provided, consider only characters in `set` to be whitespace."
  [str &opt set])

(defn string?
  {:params [:any] :ret :boolean :narrows :string}
  "(string? x)\n\nCheck if x is a string."
  [x])

(defn struct
  {:params [:any] :ret :struct}
  "(struct & kvs)\n\nCreate a new struct from a sequence of key value pairs. kvs is a sequence k1, v1, k2, v2, k3, v3, ... If kvs has an odd number of elements, an error will be thrown. Returns the new struct."
  [& kvs])

(defn struct/getproto
  {:params [:struct] :ret :struct?}
  "(struct/getproto st)\n\nReturn the prototype of a struct, or nil if it doesn't have one."
  [st])

(defn struct/proto-flatten
  {:params [:struct] :ret :struct}
  "(struct/proto-flatten st)\n\nConvert a struct with prototypes to a struct with no prototypes by merging all key value pairs from recursive prototypes into one new struct."
  [st])

(defn struct/rawget
  {:params [:struct :any] :ret :any} # TODO: the C source does not say what it returns
  "(struct/rawget st key)\n\nGets a value from a struct `st` without looking at the prototype struct. If `st` does not contain the key directly, the function will return nil without checking the prototype. Returns the value in the struct."
  [st key])

(defn struct/to-table
  {:params [:struct :boolean?] :ret :table}
  "(struct/to-table st &opt recursive)\n\nConvert a struct to a table. If recursive is true, also convert the table's prototypes into the new struct's prototypes as well."
  [st &opt recursive])

(defn struct/with-proto
  {:params [:struct? :any] :ret :struct}
  "(struct/with-proto proto & kvs)\n\nCreate a structure, as with the usual struct constructor but set the struct prototype as well."
  [proto & kvs])

(defn struct?
  {:params [:any] :ret :boolean :narrows :struct}
  "(struct? x)\n\nCheck if x is a struct."
  [x])

(defn sum
  {:params [(or [:number] @[:number])] :ret :number}
  "(sum xs)\n\nReturns the sum of xs. If xs is empty, returns 0."
  [xs])

(defn symbol
  {:params [:any] :ret :symbol}
  "(symbol & xs)\n\nCreates a symbol by concatenating the elements of `xs` together. If an element is not a byte sequence, it is converted to bytes via `describe`. Returns the new symbol."
  [& xs])

(defn symbol/slice
  {:params [(or :string :buffer :symbol :keyword) :number? :number?] :ret :symbol}
  "(symbol/slice bytes &opt start end)\n\nSame as string/slice, but returns a symbol."
  [bytes &opt start end])

(defn symbol?
  {:params [:any] :ret :boolean :narrows :symbol}
  "(symbol? x)\n\nCheck if x is a symbol."
  [x])

(defn table
  {:params [:any] :ret :table}
  "(table & kvs)\n\nCreates a new table from a variadic number of keys and values. kvs is a sequence k1, v1, k2, v2, k3, v3, ... If kvs has an odd number of elements, an error will be thrown. Returns the new table."
  [& kvs])

(defn table/clear
  {:params [:table] :ret :table}
  "(table/clear tab)\n\nRemove all key-value pairs in a table and return the modified table `tab`."
  [tab])

(defn table/clone
  {:params [:table] :ret :table}
  "(table/clone tab)\n\nCreate a copy of a table. Updates to the new table will not change the old table, and vice versa."
  [tab])

(defn table/getproto
  {:params [:table] :ret :table?}
  "(table/getproto tab)\n\nGet the prototype table of a table. Returns nil if the table has no prototype, otherwise returns the prototype."
  [tab])

(defn table/new
  {:params [:number] :ret :table}
  "(table/new capacity)\n\nCreates a new empty table with pre-allocated memory for `capacity` entries. This means that if one knows the number of entries going into a table on creation, extra memory allocation can be avoided. Returns the new table."
  [capacity])

(defn table/proto-flatten
  {:params [:table] :ret :table}
  "(table/proto-flatten tab)\n\nCreate a new table that is the result of merging all prototypes into a new table."
  [tab])

(defn table/rawget
  {:params [:table a] :ret :any} # TODO: the C source does not say what it returns
  "(table/rawget tab key)\n\nGets a value from a table `tab` without looking at the prototype table. If `tab` does not contain the key directly, the function will return nil without checking the prototype. Returns the value in the table."
  [tab key])

(defn table/setproto
  {:params [:table :table?] :ret :table}
  "(table/setproto tab proto)\n\nSet the prototype of a table. Returns the original table `tab`."
  [tab proto])

(defn table/to-struct
  {:params [:table :struct?] :ret :struct}
  "(table/to-struct tab &opt proto)\n\nConvert a table to a struct. Returns a new struct."
  [tab &opt proto])

(defn table/weak
  {:params [:number] :ret :table}
  "(table/weak capacity)\n\nCreates a new empty table with weak references to keys and values. Similar to `table/new`. Returns the new table."
  [capacity])

(defn table/weak-keys
  {:params [:number] :ret :table}
  "(table/weak-keys capacity)\n\nCreates a new empty table with weak references to keys and normal references to values. Similar to `table/new`. Returns the new table."
  [capacity])

(defn table/weak-values
  {:params [:number] :ret :table}
  "(table/weak-values capacity)\n\nCreates a new empty table with normal references to keys and weak references to values. Similar to `table/new`. Returns the new table."
  [capacity])

(defn table?
  {:params [:any] :ret :boolean :narrows :table}
  "(table? x)\n\nCheck if x is a table."
  [x])

(defmacro tabseq
  {:params [:tuple :any :any] :ret :table}
  "(tabseq head key-body & value-body)\n\nSimilar to `loop`, but accumulates key value pairs into a table.\nSee `loop` for details."
  [head key-body & value-body])

(defn take
  {:params [:number (or [a] @[a] :string :buffer)] :ret (or [a] :string)}
  "(take n ind)\n\nTake the first n elements of a fiber, indexed or bytes type. Returns a new array, tuple or string,\nrespectively. If `n` is negative, takes the last `n` elements instead."
  [n ind])

(defn take-until
  {:params [(fn [a] :any) (or [a] @[a] :string :buffer)] :ret (or [a] :string)}
  "(take-until pred ind)\n\nSame as `(take-while (complement pred) ind)`."
  [pred ind])

(defn take-while
  {:params [(fn [a] :any) (or [a] @[a] :string :buffer)] :ret (or [a] :string)}
  "(take-while pred ind)\n\nGiven a predicate, take only elements from a fiber, indexed, or bytes type that satisfy\nthe predicate, and abort on first failure. Returns a new array, tuple, or string, respectively."
  [pred ind])

(defn thaw
  {:params [:any] :ret :any}
  "(thaw ds)\n\nThaw an object (make it mutable) and do a deep copy, making\nchild values also mutable. Closures, fibers, and abstract\ntypes will not be recursively thawed, but all other types will."
  [ds])

(defn thaw-keep-keys
  {:params [:any] :ret :any}
  "(thaw-keep-keys ds)\n\nSimilar to `thaw`, but do not modify table or struct keys."
  [ds])

(defmacro toggle
  {:params [:any] :ret :boolean}
  "(toggle value)\n\nSet a value to its boolean inverse. Same as `(set value (not value))`."
  [value])

(defn trace
  {:params [:function] :ret :function}
  "(trace func)\n\nEnable tracing on a function. Returns the function."
  [func])

(defmacro tracev
  {:params [a] :ret a}
  "(tracev x)\n\nPrint to stderr a value and a description of the form that produced that value.\nEvaluates to x."
  [x])

(defn true?
  {:params [:any] :ret :boolean :narrows :any}
  "(true? x)\n\nCheck if x is true."
  [x])

(defn truthy?
  {:params [:any] :ret :boolean :narrows :any}
  "(truthy? x)\n\nCheck if x is truthy."
  [x])

(defmacro try
  {:params [:any :tuple] :ret :any}
  "(try body catch)\n\nTry something and catch errors. `body` is any expression,\nand `catch` should be a form, the first element of which is a tuple. This tuple\nshould contain a binding for errors and an optional binding for\nthe fiber wrapping the body. Returns the result of `body` if no error,\nor the result of `catch` if an error."
  [body catch])

(defn tuple
  {:params [:any] :ret :tuple}
  "(tuple & items)\n\nCreates a new tuple that contains items. Returns the new tuple."
  [& items])

(defn tuple/brackets
  {:params [:any] :ret :tuple}
  "(tuple/brackets & xs)\n\nCreates a new bracketed tuple containing the elements xs."
  [& xs])

(defn tuple/join
  {:params [(or [:any] @[:any])] :ret :tuple}
  "(tuple/join & parts)\n\nCreate a tuple by joining together other tuples and arrays."
  [& parts])

(defn tuple/setmap
  {:params [:tuple :number :number] :ret :tuple}
  "(tuple/setmap tup line column)\n\nSet the sourcemap metadata on a tuple. line and column indicate should be integers."
  [tup line column])

(defn tuple/slice
  {:params [(or [a] @[a]) :number?] :ret [a]}
  "(tuple/slice arrtup [,start=0 [,end=(length arrtup)]])\n\nTake a sub-sequence of an array or tuple from index `start` inclusive to index `end` exclusive. If `start` or `end` are not provided, they default to 0 and the length of `arrtup`, respectively. `start` and `end` can also be negative to indicate indexing from the end of the input. Note that if `start` is negative it is exclusive, and if `end` is negative it is inclusive, to allow a full negative slice range. Returns the new tuple."
  [arrtup [,start=0 [,end=(length arrtup)]]])

(defn tuple/sourcemap
  {:params [:tuple] :ret [:number :number]}
  "(tuple/sourcemap tup)\n\nReturns the sourcemap metadata attached to a tuple, which is another tuple (line, column)."
  [tup])

(defn tuple/type
  {:params [:tuple] :ret :keyword}
  "(tuple/type tup)\n\nChecks how the tuple was constructed. Will return the keyword :brackets if the tuple was parsed with brackets, and :parens otherwise. The two types of tuples will behave the same most of the time, but will print differently and be treated differently by the compiler."
  [tup])

(defn tuple?
  {:params [:any] :ret :boolean :narrows :tuple}
  "(tuple? x)\n\nCheck if x is a tuple."
  [x])

(defn type
  {:params [:any] :ret :keyword}
  "(type x)\n\nReturns the type of `x` as a keyword. `x` is one of:\n\n* :nil\n\n* :boolean\n\n* :number\n\n* :array\n\n* :tuple\n\n* :table\n\n* :struct\n\n* :string\n\n* :buffer\n\n* :symbol\n\n* :keyword\n\n* :function\n\n* :cfunction\n\n* :fiber\n\nor another keyword for an abstract type."
  [x])

(defmacro unless
  {:params [:any :any] :ret :any}
  "(unless condition & body)\n\nShorthand for `(when (not condition) ;body)`. "
  [condition & body])

(defn unmarshal
  {:params [(or :string :buffer :symbol :keyword) :table] :ret :any} # TODO: the C source does not say what it returns
  "(unmarshal buffer &opt lookup)\n\nUnmarshal a value from a buffer. An optional lookup table can be provided to allow for aliases to be resolved. Returns the value unmarshalled from the buffer."
  [buffer &opt lookup])

(defn untrace
  {:params [:function] :ret :function}
  "(untrace func)\n\nDisables tracing on a function. Returns the function."
  [func])

(defn update
  {:params [a :any (fn [:any] :any) :any] :ret a}
  "(update ds key func & args)\n\nFor a given key in data structure `ds`, replace its corresponding value with the\nresult of calling `func` on that value. If `args` are provided, they will be passed\nalong to `func` as well. Returns `ds`, updated."
  [ds key func & args])

(defn update-in
  {:params [a (or [:any] @[:any]) (fn [:any] :any) :any] :ret a}
  "(update-in ds ks f & args)\n\nUpdate a value in a nested data structure `ds`. Looks into `ds` via a sequence of keys,\nand replaces the value found there with `f` applied to that value.\nMissing data structures will be replaced with tables. Returns\nthe modified, original data structure."
  [ds ks f & args])

(defmacro use
  {:params [:any] :ret :nil}
  "(use & modules)\n\nSimilar to `import`, but imported bindings are not prefixed with a module\nidentifier. Can also import multiple modules in one shot."
  [& modules])

(defn values
  {:params [:any] :ret @[:any]}
  "(values x)\n\nGet the values of an associative data structure."
  [x])

(defmacro var-
  {:params [:symbol :any] :ret :any}
  "(var- name & more)\n\nDefine a private var that will not be exported."
  [name & more])

(defmacro varfn
  {:params [:symbol :any] :ret :function}
  "(varfn name & body)\n\nCreate a function that can be rebound. `varfn` has the same signature\nas `defn`, but defines functions in the environment as vars. If a var `name`\nalready exists in the environment, it is rebound to the new function. Returns\na function."
  [name & body])

(defn varglobal
  {:params [:symbol :any] :ret :nil}
  "(varglobal name init)\n\nDynamically create a global var."
  [name init])

(defn walk
  {:params [(fn [:any] :any) :any] :ret :any}
  "(walk f form)\n\nIterate over the values in ast and apply `f`\nto them. Collect the results in a data structure. If ast is not a\ntable, struct, array, or tuple,\nreturns form."
  [f form])

(defn warn-compile
  {:params [:string :keyword :string? :number? :number?] :ret :nil}
  "(warn-compile msg level where &opt line col)\n\nDefault handler for a compile warning."
  [msg level where &opt line col])

(defmacro when
  {:params [:any :any] :ret :any}
  "(when condition & body)\n\nEvaluates the body when the condition is true. Otherwise returns nil."
  [condition & body])

(defmacro when-let
  {:params [:tuple :any] :ret :any}
  "(when-let bindings & body)\n\nSame as `(if-let bindings (do ;body))`."
  [bindings & body])

(defmacro when-with
  {:params [:tuple :any] :ret :any}
  "(when-with [binding ctor dtor] & body)\n\nSimilar to with, but if binding is false or nil, returns\nnil without evaluating the body. Otherwise, the same as `with`."
  [[binding ctor dtor] & body])

(defmacro with
  {:params [:tuple :any] :ret :any}
  "(with [binding ctor dtor] & body)\n\nEvaluate `body` with some resource, which will be automatically cleaned up\nif there is an error in `body`. `binding` is bound to the expression `ctor`, and\n`dtor` is a function or callable that is passed the binding. If no destructor\n(`dtor`) is given, will call :close on the resource."
  [[binding ctor dtor] & body])

(defmacro with-dyns
  {:params [:tuple :any] :ret :any}
  "(with-dyns bindings & body)\n\nRun a block of code in a new fiber that has some\ndynamic bindings set. The fiber will not mask errors\nor signals, but the dynamic bindings will be properly\nunset, as dynamic bindings are fiber-local."
  [bindings & body])

(defmacro with-env
  {:params [(or :struct :table) :any] :ret :any}
  "(with-env env & body)\n\nRun a block of code with a given environment table"
  [env & body])

(defmacro with-syms
  {:params [:tuple :any] :ret :any}
  "(with-syms syms & body)\n\nEvaluates `body` with each symbol in `syms` bound to a generated, unique symbol."
  [syms & body])

(defmacro with-vars
  {:params [:tuple :any] :ret :any}
  "(with-vars vars & body)\n\nEvaluates `body` with each var in `vars` temporarily bound. Similar signature to\n`let`, but each binding must be a var."
  [vars & body])

(defn xprin
  {:params [:any :any] :ret :nil}
  "(xprin to & xs)\n\nPrint to a file or other value explicitly (no dynamic bindings). The value to print to is the first argument, and is otherwise the same as `prin`. Returns nil."
  [to & xs])

(defn xprinf
  {:params [:any :string :any] :ret :nil}
  "(xprinf to fmt & xs)\n\nLike `prinf` but prints to an explicit file or value `to`. Returns nil."
  [to fmt & xs])

(defn xprint
  {:params [:any :any] :ret :nil}
  "(xprint to & xs)\n\nPrint to a file or other value explicitly (no dynamic bindings) with a trailing newline character. The value to print to is the first argument, and is otherwise the same as `print`. Returns nil."
  [to & xs])

(defn xprintf
  {:params [:any :string :any] :ret :nil}
  "(xprintf to fmt & xs)\n\nLike `printf` but prints to an explicit file or value `to`. Returns nil."
  [to fmt & xs])

(defn yield
  {:params [:any] :ret :any}
  "(yield &opt x)\n\nYield a value to a parent fiber. When a fiber yields, its execution is paused until another thread resumes it. The fiber will then resume, and the last yield call will return the value that was passed to resume."
  [&opt x])

(defn zero?
  {:params [:number] :ret :boolean :narrows :any}
  "(zero? x)\n\nCheck if x is zero."
  [x])

(defn zipcoll
  {:params [(or [:any] @[:any]) (or [:any] @[:any])] :ret :table}
  "(zipcoll ks vs)\n\nCreates a table from two arrays/tuples.\nReturns a new table."
  [ks vs])

# What a PEG pattern is, for the specials below and for `peg/*` above: a name rather than
# `:any`, since `true` and a function are the values a pattern is not.

(def Pattern :typedef
  (or :string :buffer :number :keyword :tuple :array :struct :table :abstract))

# What `ffi/*` calls a type: a keyword like `:int`, a struct of them, or what `ffi/struct` made.

(def CType :typedef (or :keyword :symbol :tuple :array :struct :abstract))

# Special forms: Janet compiles them, so no environment holds them.

(defn break
  {:params [:any] :ret :never}
  "(break &opt value)"
  [&opt value])

(defn def
  {:params [:symbol :any a] :ret a}
  "(def name meta... value)"
  [name meta... value])

(defn do
  {:params [:any] :ret :any}
  "(do & body)"
  [& body])

(defn fn
  {:params [:symbol? :tuple :any] :ret :function}
  "(fn name? [params] & body)"
  [name? [params] & body])

(defn if
  {:params [:any a b] :ret (or a b)}
  "(if condition when-true &opt when-false)"
  [condition when-true &opt when-false])

(defn quasiquote
  {:params [:any] :ret :any}
  "(quasiquote x)"
  [x])

(defn quote
  {:params [:any] :ret :any}
  "(quote x)"
  [x])

(defn set
  {:params [:any a] :ret a}
  "(set place value)"
  [place value])

(defn splice
  {:params [:any] :ret :any}
  "(splice x)"
  [x])

(defn unquote
  {:params [:any] :ret :any}
  "(unquote x)"
  [x])

(defn upscope
  {:params [:any] :ret :any}
  "(upscope & body)"
  [& body])

(defn var
  {:params [:symbol :any a] :ret a}
  "(var name meta... value)"
  [name meta... value])

(defn while
  {:params [:any :any] :ret :nil}
  "(while condition & body)"
  [condition & body])

# PEG specials: they name patterns rather than bindings, and share names with the core
# above, so they declare themselves in a block of their own.
(comment :peg

(defn sequence
  {:params [Pattern] :ret Pattern}
  "(sequence & patts)"
  [& patts])

(defn choice
  {:params [Pattern] :ret Pattern}
  "(choice & patts)"
  [& patts])

(defn any
  {:params [Pattern] :ret Pattern}
  "(any patt)"
  [patt])

(defn some
  {:params [Pattern] :ret Pattern}
  "(some patt)"
  [patt])

(defn opt
  {:params [Pattern] :ret Pattern}
  "(opt patt)"
  [patt])

(defn between
  {:params [:number :number Pattern] :ret Pattern}
  "(between min max patt)"
  [min max patt])

(defn at-least
  {:params [:number Pattern] :ret Pattern}
  "(at-least n patt)"
  [n patt])

(defn at-most
  {:params [:number Pattern] :ret Pattern}
  "(at-most n patt)"
  [n patt])

(defn repeat
  {:params [:number Pattern] :ret Pattern}
  "(repeat n patt)"
  [n patt])

(defn range
  {:params [:string] :ret Pattern}
  "(range & ranges)"
  [& ranges])

(defn set
  {:params [:string] :ret Pattern}
  "(set chars)"
  [chars])

(defn look
  {:params [:number? Pattern] :ret Pattern}
  "(look ?offset patt)"
  [?offset patt])

(defn not
  {:params [Pattern] :ret Pattern}
  "(not patt)"
  [patt])

(defn if
  {:params [Pattern Pattern] :ret Pattern}
  "(if cond patt)"
  [cond patt])

(defn if-not
  {:params [Pattern Pattern] :ret Pattern}
  "(if-not cond patt)"
  [cond patt])

(defn to
  {:params [Pattern] :ret Pattern}
  "(to patt)"
  [patt])

(defn thru
  {:params [Pattern] :ret Pattern}
  "(thru patt)"
  [patt])

(defn til
  {:params [Pattern Pattern] :ret Pattern}
  "(til sep patt)"
  [sep patt])

(defn sub
  {:params [Pattern Pattern] :ret Pattern}
  "(sub window patt)"
  [window patt])

(defn split
  {:params [Pattern Pattern] :ret Pattern}
  "(split sep patt)"
  [sep patt])

(defn backmatch
  {:params [:keyword?] :ret Pattern}
  "(backmatch ?tag)"
  [?tag])

(defn lenprefix
  {:params [Pattern Pattern] :ret Pattern}
  "(lenprefix n patt)"
  [n patt])

(defn drop
  {:params [Pattern] :ret Pattern}
  "(drop patt)"
  [patt])

(defn only-tags
  {:params [Pattern] :ret Pattern}
  "(only-tags patt)"
  [patt])

(defn error
  {:params [Pattern?] :ret Pattern}
  "(error ?patt)"
  [?patt])

(defn capture
  {:params [Pattern :keyword?] :ret Pattern}
  "(capture patt ?tag)"
  [patt ?tag])

(defn accumulate
  {:params [Pattern :keyword?] :ret Pattern}
  "(accumulate patt ?tag)"
  [patt ?tag])

(defn group
  {:params [Pattern :keyword?] :ret Pattern}
  "(group patt ?tag)"
  [patt ?tag])

(defn replace
  {:params [Pattern :any :keyword?] :ret Pattern}
  "(replace patt subst ?tag)"
  [patt subst ?tag])

(defn cmt
  {:params [Pattern :function :keyword?] :ret Pattern}
  "(cmt patt fun ?tag)"
  [patt fun ?tag])

(defn cms
  {:params [Pattern :function :keyword?] :ret Pattern}
  "(cms patt fun ?tag)"
  [patt fun ?tag])

(defn constant
  {:params [:any :keyword?] :ret Pattern}
  "(constant value ?tag)"
  [value ?tag])

(defn argument
  {:params [:number :keyword?] :ret Pattern}
  "(argument n ?tag)"
  [n ?tag])

(defn position
  {:params [:keyword?] :ret Pattern}
  "(position ?tag)"
  [?tag])

(defn line
  {:params [:keyword?] :ret Pattern}
  "(line ?tag)"
  [?tag])

(defn column
  {:params [:keyword?] :ret Pattern}
  "(column ?tag)"
  [?tag])

(defn backref
  {:params [:keyword :keyword?] :ret Pattern}
  "(backref prev-tag ?tag)"
  [prev-tag ?tag])

(defn unref
  {:params [Pattern :keyword?] :ret Pattern}
  "(unref patt ?tag)"
  [patt ?tag])

(defn nth
  {:params [:number Pattern :keyword?] :ret Pattern}
  "(nth index patt ?tag)"
  [index patt ?tag])

(defn number
  {:params [Pattern :number? :keyword?] :ret Pattern}
  "(number patt ?base ?tag)"
  [patt ?base ?tag])

(defn int
  {:params [:number :keyword?] :ret Pattern}
  "(int width ?tag)"
  [width ?tag])

(defn int-be
  {:params [:number :keyword?] :ret Pattern}
  "(int-be width ?tag)"
  [width ?tag])

(defn uint
  {:params [:number :keyword?] :ret Pattern}
  "(uint width ?tag)"
  [width ?tag])

(defn uint-be
  {:params [:number :keyword?] :ret Pattern}
  "(uint-be width ?tag)"
  [width ?tag])

(defn debug
  {:params [] :ret Pattern}
  "(debug)"
  [])

(defn !
  {:params [Pattern] :ret Pattern}
  "(! patt)"
  [patt])

(defn $
  {:params [:keyword?] :ret Pattern}
  "($ ?tag)"
  [?tag])

(defn %
  {:params [Pattern :keyword?] :ret Pattern}
  "(% patt ?tag)"
  [patt ?tag])

(defn *
  {:params [Pattern] :ret Pattern}
  "(* & patts)"
  [& patts])

(defn +
  {:params [Pattern] :ret Pattern}
  "(+ & patts)"
  [& patts])

(defn ->
  {:params [:keyword :keyword?] :ret Pattern}
  "(-> prev-tag ?tag)"
  [prev-tag ?tag])

(defn /
  {:params [Pattern :any :keyword?] :ret Pattern}
  "(/ patt subst ?tag)"
  [patt subst ?tag])

(defn <-
  {:params [Pattern :keyword?] :ret Pattern}
  "(<- patt ?tag)"
  [patt ?tag])

(defn >
  {:params [:number? Pattern] :ret Pattern}
  "(> ?offset patt)"
  [?offset patt])

(defn ?
  {:params [Pattern] :ret Pattern}
  "(? patt)"
  [patt])

(defn ??
  {:params [] :ret Pattern}
  "(??)"
  [])

(defn quote
  {:params [Pattern :keyword?] :ret Pattern}
  "(quote patt ?tag)"
  [patt ?tag])

)
