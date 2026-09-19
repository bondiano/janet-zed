# Types of spork's most used modules, checked against spork 1.2.0 (3918802) on Janet 1.42.1:
# `json`, `http`, `path`, `sh`, `misc`, `argparse`, `test`, `schema`, `rpc`, `fmt`, `regex`,
# `temple`, `netrepl`, `ev-utils`, `stream`, `base64` and `crc`.
#
# The server writes this file out beside its own `janet-zed.exports/spork/`, so it is read like
# any other declaration a library exports and can be handed to spork upstream unchanged. Names
# are written in full, `spork/json/decode`: a file that imports the module sees them under
# whatever prefix it gave it.
#
# Every position says what belongs there: a type variable where a call is polymorphic, and, where
# a type would be a guess, `:any` with a `# TODO` beside it naming what is still open. No position
# is left untyped without one. A predicate carries `:narrows`, `:any` where it tests a value
# rather than a type and so tells a branch nothing.

# -- spork/json --------------------------------------------------------------------------------

(defn spork/json/decode
  {:params [(or :string :buffer) :boolean? :boolean?] :ret {:string :any}} # TODO: what a document holds is known only where it is read
  "Parse JSON. `keywords` makes string keys keywords, `nils` makes null nil rather than `:null`."
  [json-source &opt keywords nils])

(defn spork/json/encode
  {:params [a :string? :string? :buffer?] :ret :buffer}
  "Encode a Janet value as JSON, appended to `buf` when one is given."
  [x &opt tab newline buf])

# -- spork/path --------------------------------------------------------------------------------

(def spork/path/sep {:type :string} "Platform separator." nil)
(def spork/path/delim {:type :string} "Platform delimiter." nil)

(defn spork/path/abspath {:params [:string] :ret :string} "Coerce a path to be absolute." [path])
(defn spork/path/abspath?
  {:params [:string] :ret :boolean :narrows :any}
  "Check if a path is absolute."
  [path])
(defn spork/path/basename {:params [:string] :ret :string} "The base file name of a path." [path])
(defn spork/path/dirname {:params [:string] :ret :string} "The directory name of a path." [path])
(defn spork/path/ext {:params [:string] :ret :string?} "The file extension of a path." [path])
(defn spork/path/join {:params [:string] :ret :string} "Join path elements together." [& els])
(defn spork/path/normalize
  {:params [:string] :ret :string}
  "Remove `.`, `..` and empty elements from a path."
  [path])
(defn spork/path/parent {:params [:string] :ret :string} "The parent directory of a path." [path])
(defn spork/path/parts {:params [:string] :ret @[:string]} "Split a path into its parts." [path])
(defn spork/path/relpath
  {:params [:string :string] :ret :string}
  "The relative path between two subpaths."
  [source target])

(def spork/path/posix/sep {:type :string} "Platform separator." nil)
(def spork/path/posix/delim {:type :string} "Platform delimiter." nil)

(defn spork/path/posix/abspath
  {:params [:string] :ret :string}
  "Coerce a path to be absolute."
  [path])
(defn spork/path/posix/abspath?
  {:params [:string] :ret :boolean :narrows :any}
  "Check if a path is absolute."
  [path])
(defn spork/path/posix/basename
  {:params [:string] :ret :string}
  "The base file name of a path."
  [path])
(defn spork/path/posix/dirname
  {:params [:string] :ret :string}
  "The directory name of a path."
  [path])
(defn spork/path/posix/ext
  {:params [:string] :ret :string?}
  "The file extension of a path."
  [path])
(defn spork/path/posix/join
  {:params [:string] :ret :string}
  "Join path elements together."
  [& els])
(defn spork/path/posix/normalize
  {:params [:string] :ret :string}
  "Remove `.`, `..` and empty elements from a path."
  [path])
(defn spork/path/posix/parent
  {:params [:string] :ret :string}
  "The parent directory of a path."
  [path])
(defn spork/path/posix/parts
  {:params [:string] :ret @[:string]}
  "Split a path into its parts."
  [path])
(defn spork/path/posix/relpath
  {:params [:string :string] :ret :string}
  "The relative path between two subpaths."
  [source target])

(def spork/path/win32/sep {:type :string} "Platform separator." nil)
(def spork/path/win32/delim {:type :string} "Platform delimiter." nil)

(defn spork/path/win32/abspath
  {:params [:string] :ret :string}
  "Coerce a path to be absolute."
  [path])
(defn spork/path/win32/abspath?
  {:params [:string] :ret :boolean :narrows :any}
  "Check if a path is absolute."
  [path])
(defn spork/path/win32/basename
  {:params [:string] :ret :string}
  "The base file name of a path."
  [path])
(defn spork/path/win32/dirname
  {:params [:string] :ret :string}
  "The directory name of a path."
  [path])
(defn spork/path/win32/ext
  {:params [:string] :ret :string?}
  "The file extension of a path."
  [path])
(defn spork/path/win32/join
  {:params [:string] :ret :string}
  "Join path elements together."
  [& els])
(defn spork/path/win32/normalize
  {:params [:string] :ret :string}
  "Remove `.`, `..` and empty elements from a path."
  [path])
(defn spork/path/win32/parent
  {:params [:string] :ret :string}
  "The parent directory of a path."
  [path])
(defn spork/path/win32/parts
  {:params [:string] :ret @[:string]}
  "Split a path into its parts."
  [path])
(defn spork/path/win32/relpath
  {:params [:string :string] :ret :string}
  "The relative path between two subpaths."
  [source target])

# -- spork/sh ----------------------------------------------------------------------------------

(defn spork/sh/copy
  {:params [:string :string] :ret :number}
  "Copy a file or directory recursively, answering the exit code of the copy."
  [src dest])
(defn spork/sh/copy-file
  {:params [:string :string] :ret :nil}
  "Copy a file, creating the directories leading to the destination."
  [src-path dst-path])
(defn spork/sh/create-dirs
  {:params [:string] :ret :nil}
  "Create every directory of a path, the last segment included."
  [dir-path])
(defn spork/sh/create-dirs-to
  {:params [:string] :ret :nil}
  "Create every directory of a path but the last segment."
  [dir-path])
(defn spork/sh/devnull
  {:params [] :ret :abstract}
  "The platform's `/dev/null`, as an open file."
  [])
(defn spork/sh/escape
  {:params [:string] :ret :string}
  "The arguments as one shell-quoted string."
  [& args])
(defn spork/sh/exec
  {:params [:string] :ret :number}
  "Execute the command, answering its exit code."
  [& args])
(defn spork/sh/exec-fail
  {:params [:string] :ret :number}
  "Execute the command, raising on a non-zero exit code."
  [& args])
(defn spork/sh/exec-slurp
  {:params [:string] :ret :string}
  "Execute the command and answer its trimmed standard output, raising on a non-zero exit code."
  [& args])
(defn spork/sh/exec-slurp-all
  {:params [:string] :ret {:out :string :err :string :status :number}}
  "Execute the command and answer its trimmed output, error output and exit code."
  [& args])
(defn spork/sh/exists?
  {:params [:string] :ret :boolean :narrows :any}
  "Whether the file or directory exists, following symlinks."
  [path])
(defn spork/sh/list-all-files
  {:params [:string (or @[:string] :nil)] :ret @[:string]}
  "Every file under a directory, recursively."
  [dir &opt into])
(defn spork/sh/make-new-file
  {:params [:string :keyword?] :ret :abstract?}
  "Open a file for writing, creating the directories leading to it."
  [file-path &opt mode])
(defn spork/sh/rm
  {:params [:string] :ret :nil}
  "Remove a directory and everything under it."
  [path])
(defn spork/sh/rm-readonly
  {:params [:string] :ret :nil}
  "Like `rm`, and removes readonly files and folders on Windows too."
  [path])
(defn spork/sh/scan-directory
  {:params [:string (fn [:string] a)] :ret :nil}
  "Apply `func` to every file and directory under `dir`, depth first."
  [dir func])
(defn spork/sh/self-exe
  {:params [] :ret :string?}
  "The path to the Janet executable."
  [])
(defn spork/sh/split
  {:params [(or :string :buffer)] :ret (or @[:string] :nil)}
  "Split a string into shell-like tokens, or nil when it does not parse."
  [s])
(defn spork/sh/which
  {:params [:string (or [:string] @[:string] :nil)] :ret :string?}
  "The full path to a program, as `which` finds it."
  [name &opt paths])

# -- spork/http --------------------------------------------------------------------------------

(def spork/http/cookie-grammar
  {:type Pattern}
  "Grammar parsing a cookie header into keys and values."
  nil)
(def spork/http/query-string-grammar
  {:type Pattern}
  "Grammar parsing a query string into a table."
  nil)
(def spork/http/request-peg {:type Pattern} "PEG parsing HTTP requests." nil)
(def spork/http/response-peg {:type Pattern} "PEG parsing HTTP responses." nil)
(def spork/http/url-grammar
  {:type Pattern}
  "Grammar parsing a URL into `[scheme host port path]`."
  nil)
(def spork/http/status-messages
  {:type {:keyword :string}}
  "HTTP status codes and their status messages."
  nil)

(defn spork/http/cookies
  {:params [a] :ret :function}
  "Middleware parsing cookies into the table under `:cookies`."
  [nextmw])
(defn spork/http/logger
  {:params [a] :ret :function}
  "Middleware printing the route, the status and the elapsed time of a request."
  [nextmw])
(defn spork/http/middleware
  {:params [a] :ret :function}
  "Coerce any value to HTTP middleware."
  [x])
(defn spork/http/router
  {:params [(or :table :struct)] :ret :function}
  "Middleware dispatching on the path of the URL."
  [routes])
(defn spork/http/read-body
  {:params [(or :table :struct)] :ret :buffer?}
  "The HTTP body of a request, or nil when it has none."
  [req])
(defn spork/http/read-request
  {:params [:abstract :buffer :boolean?]
   :ret @{:headers {:string :string}
          :connection :abstract
          :buffer :buffer
          :head-size :number
          :method :string
          :path :string
          :route :string?
          :query-string :string?
          :query (or {:string (or :string :boolean)} :nil)}}
  "Read a request header from a connection. `no-query` leaves the query unparsed."
  [conn buf &opt no-query])
(defn spork/http/read-response
  {:params [:abstract :buffer]
   :ret @{:headers {:string :string}
          :connection :abstract
          :buffer :buffer
          :head-size :number
          :status :number
          :message :string}}
  "Read a response header from a connection."
  [conn buf])
(defn spork/http/request
  {:params [:string
            :string
            @{:headers (or {:string :string} :nil)
              :body (or :string :buffer :nil)
              :stream-factory :function?
              :stream-opts (or :table :struct :nil)}]
   :ret @{:head-size :number
          :headers {:string :string}
          :connection :abstract
          :buffer :buffer
          :status :number
          :message :string
          :body (or :string :buffer :nil)}}
  "Make an HTTP request and answer the response."
  [method url &keys {:headers headers
                     :stream-opts stream-opts
                     :body body
                     :stream-factory stream-factory}])
(defn spork/http/send-response
  {:params [:abstract
            (or :table :struct)
            :buffer?]
   :ret :nil}
  "Send a response over a connection, chunked when the body is not a byte sequence."
  [conn response &opt buf])
(defn spork/http/server
  {:params [:function :string? (or :string :number :nil)] :ret :abstract}
  "A simple HTTP server, bound to 0.0.0.0:8000 by default."
  [handler &opt host port])
(defn spork/http/server-handler
  {:params [:abstract :function] :ret :nil}
  "Handle one accepted connection, calling `handler` with the request table."
  [conn handler])

# -- spork/misc --------------------------------------------------------------------------------

(defn spork/misc/always
  {:params [a] :ret (fn [& b] a)}
  "A function that discards its arguments and always answers `x`."
  [x])
(defn spork/misc/second
  {:params [(or [a] @[a])] :ret a}
  "The second element of an indexed structure."
  [xs])
(defn spork/misc/third
  {:params [(or [a] @[a])] :ret a}
  "The third element of an indexed structure."
  [xs])
(defn spork/misc/penultimate
  {:params [(or [a] @[a])] :ret a}
  "The second-to-last element of an indexed structure."
  [xs])
(defn spork/misc/antepenultimate
  {:params [(or [a] @[a])] :ret a}
  "The third-to-last element of an indexed structure."
  [xs])
(defmacro spork/misc/binary-search
  {:params [a (or [a] @[a]) :function?] :ret :number}
  "The index `x` has, or would be inserted at, in a sorted structure."
  [x arr &opt <?])
(defmacro spork/misc/binary-search-by
  {:params [a (or [a] @[a]) (fn [a] b)] :ret :number}
  "The index `x` has, or would be inserted at, in a structure sorted by `f`."
  [x arr f])
(defmacro spork/misc/caperr
  {:params [a] :ret :buffer}
  "Capture the standard error output of `body`."
  [& body])
(defmacro spork/misc/capout
  {:params [a] :ret :buffer}
  "Capture the standard output of `body`."
  [& body])
(defn spork/misc/column-combine
  {:params [a :keyword (or [:keyword] @[:keyword]) (fn [@[b]] c) :boolean?] :ret a}
  "Combine several columns of a data frame into one."
  [data-frame out-column input-columns combine-row &opt drop-input-columns])
(defmacro spork/misc/cond->
  {:params [a b] :ret a}
  "Thread `val` first through the operations of the `clauses` whose conditions hold."
  [val & clauses])
(defmacro spork/misc/cond->>
  {:params [a b] :ret a}
  "Thread `val` last through the operations of the `clauses` whose conditions hold."
  [val & clauses])
(defn spork/misc/dedent
  {:params [a] :ret :string}
  "The arguments concatenated, with their common leading whitespace removed."
  [& xs])
(defmacro spork/misc/defs
  {:params [a] :ret :nil}
  "Define constants as `let` binds them, without opening a scope."
  [& bindings])
(defn spork/misc/dfs
  {:params [a (fn [b] c) :function? :function? :function? (or :table :nil)] :ret :nil}
  "Traverse a structure depth first, pre-order."
  [data visit-leaf &opt node-before node-after get-children seen])
(defmacro spork/misc/do-def
  {:params [:symbol a b] :ret a}
  "Define `c` as `d`, evaluate `body`, and answer `c`."
  [c d & body])
(defmacro spork/misc/do-var
  {:params [:symbol a b] :ret a}
  "Define the variable `v` as `d`, evaluate `body`, and answer `v`."
  [v d & body])
(defn spork/misc/format-table
  {:params [:buffer
            a
            (or [:keyword] @[:keyword] :nil)
            (or :table :struct :nil)
            (or :table :struct :nil)]
   :ret :buffer}
  "Like `print-table`, pushed into a buffer."
  [buf-into data &opt columns header-mapping column-mapping])
(defmacro spork/misc/gett
  {:params [a b] :ret :any} # TODO: what a path of keys reaches
  "Recursive `get`, with the keys written out rather than in a vector."
  [ds & keyz])
(defn spork/misc/insert-sorted
  {:params [@[a] :function a] :ret @[a]}
  "Insert elements into a sorted array, keeping it sorted by the comparator."
  [arr <? & xs])
(defn spork/misc/insert-sorted-by
  {:params [@[a] (fn [a] b) a] :ret @[a]}
  "Insert elements into a sorted array, keeping it sorted by what `f` answers."
  [arr f & xs])
(defn spork/misc/int->string
  {:params [:number :number?] :ret :string}
  "An integer written in a base, decimal by default."
  [int &opt base])
(defn spork/misc/int/
  {:params [:number] :ret :number}
  "Integer division."
  [& xs])
(defmacro spork/misc/log
  {:params [:keyword a] :ret :nil}
  "Print to the stream `level` names, when that dynamic binding is set."
  [level & args])
(defmacro spork/misc/make
  {:params [(or :table :struct) a] :ret :table}
  "A new table of the key and value pairs, with `prototype` behind it."
  [prototype & kvpairs])
(defn spork/misc/make-id
  {:params [(or :string :keyword :nil)] :ret :keyword}
  "A random printable keyword of ten bytes of entropy, under an optional prefix."
  [&opt prefix])
(defn spork/misc/map-keys
  {:params [(fn [a] b) (or :table :struct)] :ret :table}
  "A new table with `f` applied to the keys, recursively."
  [f data])
(defn spork/misc/map-keys-flat
  {:params [(fn [a] b) (or :table :struct)] :ret :table}
  "A new table with `f` applied to the keys, without recursing."
  [f data])
(defn spork/misc/map-vals
  {:params [(fn [a] b) (or :table :struct)] :ret :table}
  "A new table with `f` applied to the values."
  [f data])
(defn spork/misc/merge-sorted
  {:params [@[a] @[a] :function?] :ret @[a]}
  "Merge two sorted arrays into one that stays sorted."
  [a b &opt <?])
(defn spork/misc/merge-sorted-by
  {:params [@[a] @[a] (fn [a] b)] :ret @[a]}
  "Merge two sorted arrays into one that stays sorted by what `f` answers."
  [a b f])
(defn spork/misc/pivot
  {:params [a :keyword :keyword :keyword :function? b] :ret :table}
  "Pivot a data frame, spreading the values of one column over columns of their own."
  [data-frame row-col col-col value-col &opt reducer reduce-init])
(defn spork/misc/print-table
  {:params [a
            (or [:keyword] @[:keyword] :nil)
            (or :table :struct :nil)
            (or :table :struct :nil)]
   :ret :nil}
  "Print the rows of a structure as a padded table with a heading."
  [data &opt columns header-mapping column-mapping])
(defn spork/misc/randomize-array
  {:params [@[a] :abstract?] :ret @[a]}
  "Shuffle an array in place, with an optional random number generator."
  [arr &opt rng])
(defn spork/misc/select-keys
  {:params [(or :table :struct) (or [a] @[a])] :ret :table}
  "A new table of the selected keys of a dictionary."
  [data keyz])
(defmacro spork/misc/set*
  {:params [(or [a] @[a]) (or [b] @[b])] :ret :nil}
  "Parallel `set`: evaluate every expression, then assign them to the targets."
  [tgts exprs])
(defn spork/misc/string->int
  {:params [(or :string :buffer) :number?] :ret :number}
  "Parse an integer in a base, decimal by default, without floating point notation."
  [str &opt base])
(defn spork/misc/table-filter
  {:params [(fn [a b] :boolean) (or :table :struct)] :ret :table}
  "Filter a dictionary into a table, with `pred` taking the key and the value."
  [pred dict])
(defn spork/misc/trim-prefix
  {:params [:string :string] :ret :string}
  "The string without the prefix, when it has one."
  [prefix str])
(defn spork/misc/trim-suffix
  {:params [:string :string] :ret :string}
  "The string without the suffix, when it has one."
  [suffix str])
(defmacro spork/misc/until
  {:params [a b] :ret :nil}
  "Repeat `body` while `cnd` is false."
  [cnd & body])
(defmacro spork/misc/vars
  {:params [a] :ret :nil}
  "Define variables as `let` binds them, without opening a scope."
  [& bindings])

# -- spork/argparse ----------------------------------------------------------------------------

(defn spork/argparse/argparse
  {:params [:string (or :string :keyword :struct :table :tuple :array)] :ret :table?}
  "Parse `(dyn :args)` by the options given, or answer nil and print usage when they do not fit."
  [description &keys options])

# -- spork/test --------------------------------------------------------------------------------

(def spork/test/num-tests-passed {:type :number} "How many asserts of the suite held." nil)
(def spork/test/num-tests-run {:type :number} "How many asserts the suite ran." nil)
(def spork/test/skip-count {:type :number} "How many asserts were skipped." nil)
(def spork/test/skip-n {:type :number} "How many asserts are still to be skipped." nil)
(def spork/test/start-time {:type :number} "When the suite started, by `os/clock`." nil)
(def spork/test/suite-num
  {:type :any} # TODO: whatever `start-suite` was last handed, the current file by default
  "The name of the running suite."
  nil)

(defmacro spork/test/assert
  {:params [a b] :ret a}
  "Assert that `x` holds, reporting where it does not, and answer `x`."
  [x &opt e])
(defmacro spork/test/assert-not
  {:params [a b] :ret :boolean}
  "Assert that `x` does not hold."
  [x &opt e])
(defmacro spork/test/assert-error
  {:params [a b] :ret :boolean}
  "Assert that the forms raise an error."
  [msg & forms])
(defmacro spork/test/assert-no-error
  {:params [a b] :ret :boolean}
  "Assert that the forms raise no error."
  [msg & forms])
(defn spork/test/assert-docs
  {:params [:string] :ret :nil}
  "Assert that every public binding of the module at `path` has a proper docstring."
  [path])
(defmacro spork/test/capture-stdout
  {:params [a] :ret [a :string]}
  "Run the body, answering its value and what it printed to standard output."
  [& body])
(defmacro spork/test/capture-stderr
  {:params [a] :ret [a :string]}
  "Run the body, answering its value and what it printed to standard error."
  [& body])
(defmacro spork/test/suppress-stdout
  {:params [a] :ret a}
  "Run the body with its standard output discarded."
  [& body])
(defmacro spork/test/suppress-stderr
  {:params [a] :ret a}
  "Run the body with its standard error discarded."
  [& body])
(defn spork/test/start-suite {:params [a] :ret :number} "Start a test suite." [&opt name])
(defn spork/test/end-suite
  {:params [] :ret :nil}
  "End the suite, print a summary and exit when an assert failed."
  [])
(defn spork/test/skip-asserts {:params [:number] :ret :nil} "Skip the next `n` asserts." [n])
(defmacro spork/test/timeit
  {:params [a b] :ret a}
  "Evaluate `form`, print how long it took, and answer its value."
  [form &opt tag])
(defmacro spork/test/timeit-loop
  {:params [a b] :ret :nil}
  "Like `loop`, printing how long it took; a `:timeout` verb iterates for so many seconds."
  [head & body])

# -- spork/schema ------------------------------------------------------------------------------

(defmacro spork/schema/validator
  {:params [a] :ret (fn [b] b)}
  "A function of one argument raising where it does not fit `pattern`, and answering it where it does."
  [pattern])
(defmacro spork/schema/predicate
  {:params [a] :ret (fn [b] :boolean)}
  "A function of one argument answering whether it fits `pattern`."
  [pattern])
(defn spork/schema/make-validator
  {:params [a] :ret (fn [& b] (fn [c] c))}
  "The function form of `validator`: a thunk answering the validator, as `compile` does."
  [schema])
(defn spork/schema/make-predicate
  {:params [a] :ret (fn [& b] (fn [c] :boolean))}
  "The function form of `predicate`: a thunk answering the predicate, as `compile` does."
  [schema])

# -- spork/rpc ---------------------------------------------------------------------------------

(def spork/rpc/default-host {:type :string} "Default host to run the server on and connect to." nil)
(def spork/rpc/default-port {:type :string} "Default port to run the server on and connect to." nil)

(defn spork/rpc/server
  {:params [(or :table :struct) :string? (or :string :number :nil) :number?] :ret :abstract}
  "An RPC server calling the functions of a dictionary for its clients."
  [functions &opt host port workers-per-connection])
(defn spork/rpc/client
  {:params [:string? (or :string :number :nil) a] :ret :table}
  "An RPC client: a table of a function per remote call, and `:close`."
  [&opt host port name])

# -- spork/fmt ---------------------------------------------------------------------------------

(def spork/fmt/*user-indent-2-forms*
  {:type :keyword}
  "Dynamic binding of more forms to indent two spaces, as control forms are."
  nil)

(defn spork/fmt/format
  {:params [(or :string :buffer)] :ret :buffer}
  "Format a string of source code to a buffer."
  [source])
(defn spork/fmt/format-print
  {:params [(or :string :buffer)] :ret :nil}
  "Format a string of source code and print the result."
  [source])
(defn spork/fmt/format-file {:params [:string] :ret :nil} "Format a file in place." [file])

# -- spork/regex -------------------------------------------------------------------------------

(def spork/regex/peg {:type :abstract} "PEG turning a regular expression into PEG source." nil)

(defn spork/regex/source
  {:params [(or :string :buffer)] :ret Pattern}
  "Compile a subset of regex to PEG source code."
  [pattern])
(defn spork/regex/compile
  {:params [Pattern] :ret Pattern}
  "Compile a regex string to a PEG; a PEG is answered as it is."
  [pattern])
(defn spork/regex/match
  {:params [Pattern (or :string :buffer) :number?] :ret (or @[:any] :nil)} # TODO: what the captures hold
  "Like `peg/match`, for regexes."
  [reg text &opt start])
(defn spork/regex/find
  {:params [Pattern (or :string :buffer) :number?] :ret :number?}
  "Like `peg/find`, for regexes."
  [reg text &opt start])
(defn spork/regex/find-all
  {:params [Pattern (or :string :buffer) :number?] :ret @[:number]}
  "Like `peg/find-all`, for regexes."
  [reg text &opt start])
(defn spork/regex/replace
  {:params [Pattern a (or :string :buffer) :number?] :ret :buffer}
  "Like `peg/replace`, for regexes."
  [reg rep text &opt start])
(defn spork/regex/replace-all
  {:params [Pattern a (or :string :buffer) :number?] :ret :buffer}
  "Like `peg/replace-all`, for regexes."
  [reg rep text &opt start])

# -- spork/temple ------------------------------------------------------------------------------

(def spork/temple/base-env {:type :table} "Base environment templates are rendered in." nil)

(defn spork/temple/add-loader
  {:params [] :ret :array}
  "Add the template loader to `module/loaders` and `module/paths`."
  [])
(defn spork/temple/create
  {:params [(or :string :buffer) a] :ret :function}
  "Compile a template string into a function printing it, `where` naming its source."
  [source &opt where])
(defn spork/temple/compile
  {:params [(or :string :buffer)] :ret (fn [& a] :buffer)}
  "Compile a template into a function of `&keys` arguments answering the rendered buffer."
  [str])

# -- spork/netrepl -----------------------------------------------------------------------------

(def spork/netrepl/default-host
  {:type :string}
  "Default host to run the server on and connect to."
  nil)
(def spork/netrepl/default-port
  {:type :string}
  "Default port to run the server on and connect to."
  nil)

(defn spork/netrepl/server
  {:params [:string?
            (or :string :number :nil)
            (or :table :function :nil)
            :function?
            (or :string :function :nil)]
   :ret :abstract}
  "Start a repl server; `env` is a table, or a function making one per connection."
  [&opt host port env cleanup welcome-msg])
(defn spork/netrepl/server-single
  {:params [:string? (or :string :number :nil) :table? :function? (or :string :function :nil)]
   :ret :abstract}
  "Start a repl server of one environment shared by every connection."
  [&opt host port env cleanup welcome-msg])
(defn spork/netrepl/run-server
  {:params [:string?
            (or :string :number :nil)
            (or :table :function :nil)
            :function?
            (or :string :function :nil)]
   :ret :nil}
  "Run `server` and wait until it closes."
  [&opt host port env cleanup welcome-msg])
(defn spork/netrepl/run-server-single
  {:params [:string? (or :string :number :nil) :table? :function? (or :string :function :nil)]
   :ret :nil}
  "Run `server-single` and wait until it closes."
  [&opt host port env cleanup welcome-msg])
(defn spork/netrepl/client
  {:params [:string? (or :string :number :nil) a :function?] :ret :nil}
  "Connect to a repl server and run a repl over it until it disconnects."
  [&opt host port name connect])

# -- spork/ev-utils ----------------------------------------------------------------------------

(defn spork/ev-utils/nursery
  {:params [] :ret :table}
  "A group of fibers, for structured concurrency."
  [])
(defn spork/ev-utils/go-nursery
  {:params [:table (or :function :fiber) a] :ret :fiber}
  "Spawn a fiber into a nursery, as `ev/go` does."
  [nurse f &opt value])
(defmacro spork/ev-utils/spawn-nursery
  {:params [:table a] :ret :fiber}
  "Like `ev/spawn`, with the fiber in a nursery."
  [nurse & body])
(defn spork/ev-utils/join-nursery
  {:params [:table] :ret :nil}
  "Suspend the current fiber until the nursery is empty."
  [nurse])
(defn spork/ev-utils/pcall
  {:params [(fn [:number] a) :number] :ret :nil}
  "Call `f` `n` times in parallel, each with its fiber's index."
  [f n])
(defn spork/ev-utils/pmap
  {:params [(fn [a] b) c :number?] :ret (or @[b] @{:any b})}
  "Map `f` over `data` in parallel, `n-workers` at a time when given."
  [f data &opt n-workers])
(defn spork/ev-utils/pmap-full
  {:params [(fn [a] b) c] :ret (or @[b] @{:any b})}
  "Function form of `ev/gather`: every sibling is canceled when one raises."
  [f data])
(defn spork/ev-utils/pmap-limited
  {:params [(fn [a] b) c :number] :ret (or @[b] @{:any b})}
  "Like `pmap-full`, `n-workers` at a time."
  [f data n-workers])
(defn spork/ev-utils/pdag
  {:params [(fn [a] b) (or :table :struct) :number?] :ret @{:any b}}
  "Call `f` on every node of a graph, each after its children, answering the results by node."
  [f dag &opt n-workers])
(defn spork/ev-utils/multithread-service
  {:params [(or :function :fiber) :number] :ret :abstract}
  "Run `thread-main` on `n-threads` threads, restarting a thread that fails."
  [thread-main n-threads])
(defmacro spork/ev-utils/wait-cancel
  {:params [a] :ret :never}
  "Wait until the current fiber is canceled, then run the body."
  [& body])

# -- spork/stream ------------------------------------------------------------------------------

(defn spork/stream/lines
  {:params [:abstract (or :string :buffer)] :ret :fiber}
  "A fiber yielding each line of a stream, split by `separator`, `\\n` by default."
  [stream &named separator])
(defn spork/stream/make-stdin {:params [] :ret :abstract} "A readable stream on /dev/stdin." [])
(defn spork/stream/make-stdout {:params [] :ret :abstract} "A writable stream on /dev/stdout." [])
(defn spork/stream/make-stderr {:params [] :ret :abstract} "A writable stream on /dev/stderr." [])

# -- spork/base64 ------------------------------------------------------------------------------

(defn spork/base64/encode {:params [:string] :ret :string} "Encode a string in Base64." [x])
(defn spork/base64/decode {:params [:string] :ret :string} "Decode a string from Base64." [x])

# -- spork/crc ---------------------------------------------------------------------------------

(defn spork/crc/make-variant
  {:params [:number :number :number? a :number?] :ret :abstract}
  "A CRC function of a polynomial, an initial value, whether bytes are flipped and an output xor."
  [size polynomial &opt init byte-flip xorout])
(defn spork/crc/named-variant
  {:params [:keyword] :ret :abstract}
  "A named CRC variant, as `:crc32`."
  [name])
