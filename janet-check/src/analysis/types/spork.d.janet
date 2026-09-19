# Types of spork's most used modules, checked against spork 1.2.0 (3918802) on Janet 1.42.1:
# `json`, `http`, `path`, `sh`, `misc`, `argparse`, `test`, `schema`, `rpc`, `fmt`, `regex`,
# `temple`, `netrepl`, `ev-utils`, `stream`, `base64`, `crc`, `htmlgen`, `rawterm`, `getline`,
# `generators`, `data`, `randgen`, `utf8`, `cron`, `msg`, `channel`, `date`, `math`, `cc` and `pm`.
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

# -- spork/htmlgen -----------------------------------------------------------------------------

(def spork/htmlgen/doctype-html
  {:type :function}
  "The HTML5 doctype, as `raw` splices it."
  nil)

(defn spork/htmlgen/escape
  {:params [a] :ret :string}
  "Escape the characters of a value, as a string, for HTML."
  [x])
(defn spork/htmlgen/html
  {:params [a :buffer?] :ret :buffer}
  "Render HTML from tuples, structs and strings, into `buf` when one is given."
  [data &opt buf])
(defn spork/htmlgen/raw
  {:params [(or :string :buffer)] :ret (fn [:buffer] :buffer)}
  "Splice `text` into the HTML unescaped."
  [text])

# -- spork/rawterm -----------------------------------------------------------------------------

(defn spork/rawterm/begin
  {:params [(or (fn [:number :number] a) :nil)] :ret :abstract}
  "Put the terminal in raw mode, answering the stream its input is read from."
  [&opt on-winch])
(defn spork/rawterm/buffer-traverse
  {:params [(or :string :buffer) :number :number :boolean?] :ret :number}
  "The index `delta` codepoints away from `index`, zero-width ones skipped when asked."
  [bytes index delta &opt skip-zerowidth])
(defn spork/rawterm/ctrl-z {:params [] :ret :nil} "Suspend the process, as ctrl-z does." [])
(defn spork/rawterm/end {:params [] :ret :nil} "Leave raw mode." [])
(defn spork/rawterm/getch
  {:params [:buffer?] :ret :buffer}
  "A byte of input from standard input, into `into` when one is given."
  [&opt into])
(defn spork/rawterm/isatty
  {:params [] :ret :boolean}
  "Whether standard input is a terminal."
  [])
(defn spork/rawterm/monowidth
  {:params [(or :string :buffer) :number? :number?] :ret :number}
  "The monospace width of a string."
  [bytes &opt start-index end-index])
(defn spork/rawterm/rune-monowidth
  {:params [:number] :ret :number}
  "The monospace width of a rune: 0, 1 or 2."
  [rune])
(defn spork/rawterm/size
  {:params [] :ret [:number :number]}
  "The rows and columns the terminal shows."
  [])
(defn spork/rawterm/slice-monowidth
  {:params [(or :string :buffer) :number :number? :buffer?] :ret :buffer}
  "The bytes of a string that fit in `columns`."
  [bytes columns &opt start-index into])

# -- spork/getline -----------------------------------------------------------------------------

(def spork/getline/max-history {:type :number} "How many lines the history keeps." nil)

(defn spork/getline/default-autocomplete-context
  {:params [:buffer :number] :ret (or @[:number :string] :nil)}
  "The position and the symbol prefix before the cursor that completion works from."
  [buf pos])
(defn spork/getline/default-autocomplete-options
  {:params [:string] :ret @[:symbol]}
  "The symbols of the current environment that start with `prefix`, sorted."
  [prefix &])
(defn spork/getline/default-doc-fetch
  {:params [(or :string :symbol) :number] :ret :string?}
  "The docstring of a root binding formatted `w` columns wide, for ctrl-g."
  [sym w &])
(defn spork/getline/make-getline
  {:params [(or (fn [:buffer :number] b) :nil)
            (or (fn [:string] c) :nil)
            (or (fn [:string :number] d) :nil)]
   :ret :function}
  "A `getline` that completes and looks up docs by the handlers given."
  [&opt autocomplete-context autocomplete-options doc-fetch])

# -- spork/generators --------------------------------------------------------------------------

(defn spork/generators/concat
  {:params [a] :ret :fiber}
  "A coroutine yielding the elements of each iterable in turn."
  [& iterables])
(defn spork/generators/cycle
  {:params [a] :ret :fiber}
  "A coroutine yielding the elements of `iterable` over and over."
  [iterable])
(defn spork/generators/drop
  {:params [:number a] :ret :fiber}
  "A coroutine yielding what is left of `iterable` after its first `n` elements."
  [n iterable])
(defn spork/generators/drop-until
  {:params [(fn [b] c) a] :ret :fiber}
  "A coroutine yielding the elements of `iterable` from the first `pred` holds for."
  [pred iterable])
(defn spork/generators/drop-while
  {:params [(fn [b] c) a] :ret :fiber}
  "A coroutine yielding the elements of `iterable` from the first `pred` fails for."
  [pred iterable])
(defn spork/generators/filter
  {:params [(fn [b] c) a] :ret :fiber}
  "A coroutine yielding the elements of `iterable` that `pred` holds for."
  [pred iterable])
(defn spork/generators/from-iterable
  {:params [a] :ret :fiber}
  "A coroutine yielding the elements of `iterable`."
  [iterable])
(defn spork/generators/interleave
  {:params [a b] :ret :fiber}
  "A coroutine yielding the first element of each iterable, then the second, and so on."
  [iterable & iterables])
(defn spork/generators/interpose
  {:params [b a] :ret :fiber}
  "A coroutine yielding the elements of `iterable` with `sep` between them."
  [sep iterable])
(defn spork/generators/keep
  {:params [:function a b] :ret :fiber}
  "A coroutine yielding what `pred` answers truly for the elements of the iterables in step."
  [pred iterable & iterables])
(defn spork/generators/map
  {:params [:function a b] :ret :fiber}
  "A coroutine yielding what `f` answers for the elements of the iterables in step."
  [f iterable & iterables])
(defn spork/generators/mapcat
  {:params [:function a b] :ret :fiber}
  "A coroutine yielding the elements of what `f` answers for the elements of the iterables."
  [f iterable & iterables])
(defn spork/generators/partition
  {:params [:number a] :ret :fiber}
  "A coroutine yielding the elements of `iterable` in arrays of `n`."
  [n iterable])
(defn spork/generators/partition-by
  {:params [(fn [b] c) a] :ret :fiber}
  "A coroutine yielding runs of the elements of `iterable` that `f` answers the same for."
  [f iterable])
(defn spork/generators/range
  {:params [:number :number :number?] :ret :fiber}
  "A coroutine yielding the numbers from `from` up to `to`, by `step`."
  [from to &opt step])
(defn spork/generators/run
  {:params [a] :ret :nil}
  "Run through `iterable` for what it does."
  [iterable])
(defn spork/generators/take
  {:params [:number a] :ret :fiber}
  "A coroutine yielding the first `n` elements of `iterable`."
  [n iterable])
(defn spork/generators/take-until
  {:params [(fn [b] c) a] :ret :fiber}
  "A coroutine yielding the elements of `iterable` until `pred` holds."
  [pred iterable])
(defn spork/generators/take-while
  {:params [(fn [b] c) a] :ret :fiber}
  "A coroutine yielding the elements of `iterable` while `pred` holds."
  [pred iterable])
(defn spork/generators/to-array
  {:params [a] :ret @[:any]} # TODO: what the iterable yields
  "The elements of `iterable`, in a new array."
  [iterable])

# -- spork/data --------------------------------------------------------------------------------

(defn spork/data/diff
  {:params [a b] :ret @[:any]} # TODO: the parts of `a` and `b`, in their shapes
  "Compare two values recursively: what is only in `a`, only in `b`, and in both."
  [a b])

# -- spork/randgen -----------------------------------------------------------------------------

(def spork/randgen/*rng* {:type :keyword} "The dynamic binding of the RNG draws come from." nil)

(defn spork/randgen/rand-cdf
  {:params [(or [:number] @[:number])] :ret :number}
  "A random index, weighted by a discrete cumulative distribution."
  [cdf])
(defmacro spork/randgen/rand-cdf-path
  {:params [(or [:number] @[:number]) a] :ret :any} # TODO: whatever the path taken answers
  "Run one of the paths at random, weighted by a discrete cumulative distribution."
  [cdf & paths])
(defn spork/randgen/rand-gaussian
  {:params [:number? :number?] :ret :number}
  "A draw from the Gaussian distribution of mean `m` and standard deviation `sd`."
  [&opt m sd])
(defn spork/randgen/rand-index
  {:params [(or :tuple :array :string :buffer)] :ret :number}
  "A random index of an indexed structure."
  [xs])
(defn spork/randgen/rand-int
  {:params [:number :number] :ret :number}
  "A random integer in `[start, end)`."
  [start end])
(defmacro spork/randgen/rand-path
  {:params [a] :ret :any} # TODO: whatever the path taken answers
  "Run one of the paths at random."
  [& paths])
(defn spork/randgen/rand-uniform {:params [] :ret :number} "A random number in `[0, 1)`." [])
(defn spork/randgen/rand-value
  {:params [(or [a] @[a])] :ret a}
  "A random element of an indexed structure."
  [xs])
(defn spork/randgen/rand-weights
  {:params [(or [:number] @[:number])] :ret :number}
  "A random index, weighted by `weights`."
  [weights])
(defmacro spork/randgen/rand-weights-path
  {:params [(or [:number] @[:number]) a] :ret :any} # TODO: whatever the path taken answers
  "Run one of the paths at random, weighted by `weights`."
  [weights & paths])
(defn spork/randgen/sample-n
  {:params [(fn [] a) :number] :ret @[a]}
  "`n` draws of the sampler `f`."
  [f n])
(defn spork/randgen/set-seed
  {:params [:number] :ret :abstract}
  "Seed the RNG draws come from, answering the new RNG."
  [seed])
(defn spork/randgen/weights-to-cdf
  {:params [(or [:number] @[:number])] :ret @[:number]}
  "The cumulative distribution of `weights`, which `rand-cdf` draws from faster."
  [weights])

# -- spork/utf8 --------------------------------------------------------------------------------

(defn spork/utf8/decode-rune
  {:params [(or :string :buffer) :number?] :ret [:number? :number]}
  "The codepoint at `start` and how many bytes it takes, or `[nil 0]` past the end or malformed."
  [buf &opt start])
(defn spork/utf8/encode-rune
  {:params [:number :buffer?] :ret :buffer}
  "Push the UTF-8 encoding of a codepoint onto `buf`, or a new buffer."
  [rune &opt buf])
(defn spork/utf8/prefix->width
  {:params [:number] :ret :number}
  "How many bytes the codepoint a UTF-8 first byte starts takes."
  [c])

# -- spork/cron --------------------------------------------------------------------------------

(defn spork/cron/check
  {:params [(or :string :buffer :tuple) :number? :boolean?] :ret :boolean}
  "Whether a time, now by default, matches a cron schedule."
  [cron &opt time local])
(defn spork/cron/next-timestamp
  {:params [(or :string :buffer :tuple) :number? :boolean?] :ret :number}
  "The first time after `time`, now by default, that a cron schedule matches."
  [cron &opt time local])
(defn spork/cron/parse-cron
  {:params [(or :string :buffer)] :ret :tuple}
  "Parse a cron string into the schedule `check` reads."
  [str])

# -- spork/msg ---------------------------------------------------------------------------------

(defn spork/msg/make-proto
  {:params [:abstract (or (fn [a] (or :string :buffer)) :nil) (or (fn [:buffer] b) :nil)]
   :ret [(fn [a] :nil) (fn [] b)]}
  "The sender and the receiver of messages over a stream, as `make-send` and `make-recv` make them."
  [stream &opt pack unpack])
(defn spork/msg/make-recv
  {:params [:abstract (or (fn [:buffer] b) :nil)] :ret (fn [] b)}
  "A function that reads the next message from a stream, nil once it is closed."
  [stream &opt unpack])
(defn spork/msg/make-send
  {:params [:abstract (or (fn [a] (or :string :buffer)) :nil)] :ret (fn [a] :nil)}
  "A function that writes a message to a stream, length first."
  [stream &opt pack])

# -- spork/channel -----------------------------------------------------------------------------

(defn spork/channel/from-each
  {:params [a :fiber?] :ret :abstract}
  "A channel giving each element of `iterable`, fed by tasks under `supervisor`."
  [iterable &named supervisor])

# -- spork/date --------------------------------------------------------------------------------

(defn spork/date/add
  {:params [:struct :number? :number? :number? :number? :number? :number?] :ret :struct}
  "The date moved forward by the time given."
  [date &named years months days hours minutes seconds])
(defn spork/date/assert-date
  {:params [:struct] :ret :struct}
  "The date, or an error when it is not well formed."
  [date])
(defn spork/date/between?
  {:params [:struct :struct :struct] :ret :boolean :narrows :any}
  "Whether `date` falls between `start` and `end`."
  [date start end])
(defn spork/date/compare-dates
  {:params [:struct :struct] :ret :number}
  "-1, 0 or 1 as `d1` comes before, with or after `d2`."
  [d1 d2])
(defn spork/date/date?
  {:params [a] :ret :boolean :narrows :any}
  "Whether a value is a well-formed date, as `os/date` answers one."
  [date])
(defn spork/date/diff
  {:params [:struct :struct] :ret :number}
  "The seconds from `earlier-date` to `later-date`."
  [later-date earlier-date])
(defn spork/date/from-string
  {:params [:string :string] :ret :struct}
  "A date read from a string by a format such as `yyyy-MM-dd`."
  [date-str format-str])
(defn spork/date/gt
  {:params [:struct] :ret :boolean}
  "Whether the dates are in descending order."
  [& dates])
(defn spork/date/leap-year?
  {:params [:number] :ret :boolean :narrows :any}
  "Whether a year is a leap year of the Gregorian calendar."
  [year])
(defn spork/date/local-now {:params [] :ret :struct} "The date now, in the local time zone." [])
(defn spork/date/lt
  {:params [:struct] :ret :boolean}
  "Whether the dates are in ascending order."
  [& dates])
(defn spork/date/sub
  {:params [:struct :number? :number? :number? :number? :number? :number?] :ret :struct}
  "The date moved back by the time given."
  [date &named years months days hours minutes seconds])
(defn spork/date/to-string
  {:params [:struct :string] :ret :string}
  "A date written out by a format such as `yyyy-MM-dd`."
  [date format-str])
(defn spork/date/utc-now {:params [] :ret :struct} "The date now, in UTC." [])

# -- spork/math --------------------------------------------------------------------------------

(def spork/math/epsilon {:type :number} "Epsilon constant." nil)
(def spork/math/chi-squared-distribution-table
  {:type {:number {:number :number}}}
  "Chi squared critical values, by degrees of freedom and then by probability."
  nil)
(def spork/math/standard-normal-table
  {:type [:number]}
  "The computed standard normal table."
  nil)

(defn spork/math/add
  {:params [@[@[:number]] (or :number @[@[:number]])] :ret @[@[:number]]}
  "Add a scalar or a matrix to the matrix `m`, in place."
  [m a])
(defn spork/math/add-to-mean
  {:params [:number :number :number] :ret :number}
  "The mean `m` of `n` values with `v` added to them."
  [m n v])
(defn spork/math/approx-eq
  {:params [:number :number :number?] :ret :boolean}
  "Whether `a` equals the expected `e` within the tolerance `t`, `epsilon` by default."
  [a e &opt t])
(defn spork/math/bernoulli-distribution
  {:params [:number] :ret [:number]}
  "The Bernoulli distribution of the probability `p`."
  [p])
(defn spork/math/binominal-coeficient
  {:params [:number :number] :ret :number}
  "The binomial coefficient of a set of size `n` and a sample of size `k`."
  [n k])
(defn spork/math/binominal-distribution
  {:params [:number :number] :ret [:number]}
  "The binomial distribution of `t` trials of the probability `p`."
  [t p])
(defn spork/math/check-probability
  {:params [:number] :ret :boolean}
  "Assert that the probability `p` is between 0 and 1."
  [p])
(defn spork/math/cols {:params [@[@[:number]]] :ret :number} "The columns of a matrix." [m])
(defn spork/math/copy {:params [(or [a] @[a])] :ret @[a]} "A copy of an array or view." [xs])
(defn spork/math/cumulative-std-normal-probability
  {:params [:number] :ret :number}
  "The standard normal probability of `z`."
  [z])
(defn spork/math/det {:params [@[@[:number]]] :ret :number} "The determinant of a matrix." [m])
(defn spork/math/dot
  {:params [(or [:number] @[:number]) (or [:number] @[:number])] :ret :number}
  "The dot product of two row vectors."
  [v1 v2])
(defn spork/math/dot-fast
  {:params [(or [:number] @[:number]) (or [:number] @[:number])] :ret :number}
  "The dot product of two row vectors of equal size."
  [v1 v2])
(defn spork/math/expand-m
  {:params [:number @[@[:number]]] :ret @[@[:number]]}
  "The matrix `m` embedded in an identity matrix of size `n`."
  [n m])
(defn spork/math/extent
  {:params [(or [:number] @[:number])] :ret [:number :number]}
  "The least and the greatest number of `xs`."
  [xs])
(defn spork/math/factor {:params [:number] :ret @[:number]} "The prime factors of `n`." [n])
(defn spork/math/factorial {:params [:number] :ret :number} "The factorial of `n`." [n])
(defn spork/math/fliplr
  {:params [@[@[:number]]] :ret @[@[:number]]}
  "Flip a matrix left to right, in place."
  [m])
(defn spork/math/flipud
  {:params [@[@[:number]]] :ret @[@[:number]]}
  "Flip a matrix upside down, in place."
  [m])
(defn spork/math/geometric-mean
  {:params [(or [:number] @[:number])] :ret :number}
  "The geometric mean of `xs`."
  [xs])
(defmacro spork/math/get-only-el
  {:params [a] :ret :number}
  "The first element of the first row of the matrix `m`."
  [m])
(defn spork/math/harmonic-mean
  {:params [(or [:number] @[:number])] :ret :number}
  "The harmonic mean of `xs`."
  [xs])
(defn spork/math/ident
  {:params [:number] :ret @[@[:number]]}
  "An identity matrix of `c` by `c`."
  [c])
(defn spork/math/interquartile-range
  {:params [(or [:number] @[:number])] :ret :number}
  "The interquartile range of `xs`."
  [xs])
(defn spork/math/invmod
  {:params [:number :number] :ret :number}
  "The modular multiplicative inverse of `a` mod `m`, NaN when there is none."
  [a m])
(defn spork/math/jacobi
  {:params [:number :number] :ret :number}
  "The Jacobi symbol (a|m)."
  [a m])
(defn spork/math/join-cols
  {:params [@[@[:number]] @[@[:number]]] :ret @[@[:number]]}
  "The columns of two matrices side by side."
  [m1 m2])
(defn spork/math/join-rows
  {:params [@[@[:number]] @[@[:number]]] :ret @[@[:number]]}
  "The rows of two matrices one above the other."
  [m1 m2])
(defn spork/math/linear-regression
  {:params [(or [[:number]] @[@[:number]])] :ret {:m :number :b :number}}
  "The slope `:m` and y-intercept `:b` of the line through a set of coordinates."
  [coords])
(defn spork/math/linear-regression-line
  {:params [{:m :number :b :number}] :ret (fn [:number] :number)}
  "The function of the line `linear-regression` answered."
  [{:m m :b b}])
(defn spork/math/m-approx=
  {:params [@[@[:number]] @[@[:number]] :number?] :ret :boolean}
  "Whether two matrices of equal size are equal within the tolerance."
  [m1 m2 &opt tolerance])
(defn spork/math/matmul
  {:params [@[@[:number]] @[@[:number]]] :ret @[@[:number]]}
  "The product of two matrices, neither mutated."
  [ma mb])
(defn spork/math/median
  {:params [(or [:number] @[:number])] :ret :number}
  "The median of `xs`."
  [xs])
(defn spork/math/median-absolute-deviation
  {:params [(or [:number] @[:number])] :ret :number}
  "The median absolute deviation of `xs`."
  [xs])
(defn spork/math/minor
  {:params [@[@[:number]] :number :number] :ret @[@[:number]]}
  "The minor of the matrix `m` at `x`, `y`."
  [m x y])
(defn spork/math/mode {:params [(or [a] @[a])] :ret a} "The most frequent value of `xs`." [xs])
(defn spork/math/mop
  {:params [@[@[:number]] (fn [:number :number] :number) @[@[:number]]] :ret @[@[:number]]}
  "Update every cell of the matrix `m` with `op` and the cell of `a` at its place, in place."
  [m op a])
(defn spork/math/mul
  {:params [@[@[:number]] (or :number @[:number] @[@[:number]])] :ret @[@[:number]]}
  "Multiply the matrix `m` by a scalar, a column vector or a matrix, in place."
  [m a])
(defn spork/math/mulmod
  {:params [:number :number :number] :ret :number}
  "The product of `a` and `b` mod `m`."
  [a b m])
(defn spork/math/next-prime
  {:params [:number] :ret :number}
  "The next prime strictly greater than `n`."
  [n])
(defn spork/math/normalize-v
  {:params [(or [:number] @[:number])] :ret @[:number]}
  "The vector `xs` normalized by its Euclidean norm."
  [xs])
(defn spork/math/outer
  {:params [(or [:number] @[:number]) (or [:number] @[:number])] :ret @[@[:number]]}
  "The outer product of two vectors."
  [v1 v2])
(defn spork/math/perm {:params [@[@[:number]]] :ret :number} "The permanent of a matrix." [m])
(defn spork/math/permutation-test
  {:params [(or [:number] @[:number])
            (or [:number] @[:number])
            (or (enum :two-side :greater :lesser) :nil)
            :number?]
   :ret :number}
  "The p-value of a permutation test of whether `xs` and `ys` differ."
  [xs ys &opt a k])
(defn spork/math/permutations
  {:params [@[a] :number?] :ret @[@[a]]}
  "The permutations of length `k` of the members of `s`, which it reorders."
  [s &opt k])
(defn spork/math/poisson-distribution
  {:params [:number] :ret [:number]}
  "The Poisson distribution of `lambda`."
  [lambda])
(defn spork/math/powmod
  {:params [:number :number :number] :ret :number}
  "`a` to the power of `b` mod `m`."
  [a b m])
(defn spork/math/prime?
  {:params [:number] :ret :boolean :narrows :any}
  "Whether `n` is prime, deterministically below 2^63."
  [n])
(defn spork/math/primes {:params [] :ret :fiber} "A fiber yielding every prime." [])
(defn spork/math/qr
  {:params [@[@[:number]]] :ret {:Q @[@[:number]] :R @[@[:number]]}}
  "The QR decomposition of a matrix, by Householder transformations."
  [m])
(defn spork/math/qr1
  {:params [@[@[:number]]] :ret {:Q @[@[:number]] :m^ @[@[:number]]}}
  "One step of Householder reflections."
  [m])
(defn spork/math/quantile
  {:params [(or [:number] @[:number]) :number] :ret :number}
  "The quantile of the unsorted `xs` at `p`."
  [xs p])
(defn spork/math/quantile-rank
  {:params [(or [:number] @[:number]) :number] :ret :number}
  "The quantile rank of the value `p` in the unsorted `xs`."
  [xs p])
(defn spork/math/quantile-rank-sorted
  {:params [(or [:number] @[:number]) :number] :ret :number}
  "The quantile rank of the value `v` in the sorted `xs`."
  [xs v])
(defn spork/math/quantile-sorted
  {:params [(or [:number] @[:number]) :number] :ret :number}
  "The quantile of the sorted `xs` at `p`."
  [xs p])
(defn spork/math/quickselect
  {:params [@[:number] :number :number? :number?] :ret :nil}
  "Rearrange `arr` in place so that the `k`-th element is in its sorted place."
  [arr k &opt left right])
(defn spork/math/relative-err
  {:params [:number :number] :ret :number}
  "The relative error of `a` against the expected `e`."
  [a e])
(defn spork/math/root-mean-square
  {:params [(or [:number] @[:number])] :ret :number}
  "The root mean square of `xs`."
  [xs])
(defn spork/math/row->col
  {:params [(or @[:number] @[@[:number]])] :ret (or @[@[:number]] :nil)}
  "A row vector as a column vector; a matrix as it is."
  [xs])
(defn spork/math/rows {:params [@[@[:number]]] :ret :number} "The rows of a matrix." [m])
(defn spork/math/sample-correlation
  {:params [(or [:number] @[:number]) (or [:number] @[:number])] :ret :number}
  "The sample correlation of `xs` and `ys`."
  [xs ys])
(defn spork/math/sample-covariance
  {:params [(or [:number] @[:number]) (or [:number] @[:number])] :ret :number}
  "The sample covariance of `xs` and `ys`."
  [xs ys])
(defn spork/math/sample-skewness
  {:params [(or [:number] @[:number])] :ret :number}
  "The sample skewness of `xs`."
  [xs])
(defn spork/math/sample-standard-deviation
  {:params [(or [:number] @[:number])] :ret :number}
  "The sample standard deviation of `xs`."
  [xs])
(defn spork/math/sample-variance
  {:params [(or [:number] @[:number])] :ret :number}
  "The sample variance of `xs`."
  [xs])
(defn spork/math/scalar
  {:params [:number :number] :ret @[@[:number]]}
  "A `c` by `c` matrix with `s` on its diagonal."
  [c s])
(defn spork/math/scale
  {:params [(or [:number] @[:number]) :number] :ret @[:number]}
  "The vector `v` scaled by `k`."
  [v k])
(defn spork/math/shuffle-in-place
  {:params [@[a] :abstract?] :ret @[a]}
  "Shuffle the array `xs` in place, with an optional random number generator."
  [xs &opt rng])
(defn spork/math/sign {:params [:number] :ret :number} "The sign of `x`: -1, 0 or 1." [x])
(defn spork/math/size
  {:params [@[@[:number]]] :ret [:number :number]}
  "The rows and columns of a matrix."
  [m])
(defn spork/math/slice-m
  {:params [@[@[:number]] (or [:number] @[:number]) (or [:number] @[:number])]
   :ret @[@[:number]]}
  "The matrix `m` sliced by the `array/slice` arguments for its rows and its columns."
  [m rslice cslice])
(defn spork/math/sop
  {:params [@[@[:number]] :function :number] :ret @[@[:number]]}
  "Update every cell of the matrix `m` with `op` and the arguments `a`, in place."
  [m op & a])
(defn spork/math/squeeze
  {:params [@[@[:number]]] :ret @[:number]}
  "The rows of a matrix concatenated into one."
  [m])
(defn spork/math/standard-deviation
  {:params [(or [:number] @[:number])] :ret :number}
  "The standard deviation of `xs`."
  [xs])
(defn spork/math/subtract
  {:params [(or [:number] @[:number]) (or [:number] @[:number])] :ret @[:number]}
  "The vector `v2` subtracted from `v1` element by element."
  [v1 v2])
(defn spork/math/sum-compensated
  {:params [(or [:number] @[:number])] :ret :number}
  "The sum of `xs` by the Kahan-Babushka algorithm."
  [xs])
(defn spork/math/sum-nth-power-deviations
  {:params [(or [:number] @[:number]) :number] :ret :number}
  "The sum of the deviations of `xs` to the power `n`."
  [xs n])
(defn spork/math/svd
  {:params [@[@[:number]] :number?]
   :ret {:U @[@[:number]] :S @[@[:number]] :V @[@[:number]]}}
  "The singular value decomposition of a matrix, by repeated QR decomposition."
  [m &opt n-iter])
(defn spork/math/swap
  {:params [@[a] :number :number] :ret (or @[a] :nil)}
  "Swap the members at `i` and `j` of `arr`, in place; nil when they are the same."
  [arr i j])
(defn spork/math/t-test
  {:params [(or [:number] @[:number]) :number] :ret :number}
  "A one sample t-test of the mean of `xs` against the known `expv`."
  [xs expv])
(defn spork/math/t-test-2
  {:params [(or [:number] @[:number]) (or [:number] @[:number]) :number?] :ret :number}
  "A two sample t-test of `xs` and `ys`, with the difference `d`, 0 by default."
  [xs ys &opt d])
(defn spork/math/trans
  {:params [@[@[:number]]] :ret @[@[:number]]}
  "The transpose of a list of row vectors."
  [m])
(defn spork/math/unit-e
  {:params [:number :number] :ret @[:number]}
  "The unit vector of `n` dimensions along the dimension `k`."
  [n k])
(defn spork/math/variance
  {:params [(or [:number] @[:number])] :ret :number}
  "The variance of `xs`."
  [xs])
(defn spork/math/z-score
  {:params [:number :number :number] :ret :number}
  "The standard score of `x` for the mean `m` and the standard deviation `d`."
  [x m d])
(defn spork/math/zero
  {:params [:number :number?] :ret (or @[:number] @[@[:number]])}
  "A vector of `c` zeros, or a matrix of `r` of them when `r` is given."
  [c &opt r])

# -- spork/cc ----------------------------------------------------------------------------------

(def spork/cc/*ar* {:type :keyword} "Archiver, `ar` by default." nil)
(def spork/cc/*build-dir* {:type :keyword} "Where intermediate files go." nil)
(def spork/cc/*build-type* {:type :keyword} "`:release`, `:develop` or `:debug` presets." nil)
(def spork/cc/*c++* {:type :keyword} "C++ compiler, `c++` by default." nil)
(def spork/cc/*c++-std* {:type :keyword} "C++ standard as a two digit number." nil)
(def spork/cc/*c++flags* {:type :keyword} "Extra C++ compiler flags." nil)
(def spork/cc/*c-std* {:type :keyword} "C standard as a two digit number." nil)
(def spork/cc/*cc* {:type :keyword} "C compiler, `cc` by default." nil)
(def spork/cc/*cflags* {:type :keyword} "Extra C compiler flags." nil)
(def spork/cc/*defines* {:type :keyword} "Map of extra defines." nil)
(def spork/cc/*dynamic-libs* {:type :keyword} "Dynamic libraries to link." nil)
(def spork/cc/*janet-prefix* {:type :keyword} "Where libjanet and janet.h are found." nil)
(def spork/cc/*lflags* {:type :keyword} "Extra linker flags." nil)
(def spork/cc/*libs* {:type :keyword} "Libraries to link, static or dynamic." nil)
(def spork/cc/*msvc-cpath* {:type :keyword} "Path to Janet libraries and headers for MSVC." nil)
(def spork/cc/*msvc-libs* {:type :keyword} ".lib libraries to link with MSVC." nil)
(def spork/cc/*msvc-vcvars* {:type :keyword} "Path to vcvarsall.bat." nil)
(def spork/cc/*pkg-config-flags* {:type :keyword} "Extra flags for pkg-config." nil)
(def spork/cc/*rules* {:type :keyword} "Rules `visit-add-rule` adds to." nil)
(def spork/cc/*smart-libs* {:type :keyword} "Group libraries so the linker resolves their order." nil)
(def spork/cc/*static-libs* {:type :keyword} "Static libraries to link." nil)
(def spork/cc/*target-os* {:type :keyword} "Operating system the toolchain targets." nil)
(def spork/cc/*use-rdynamic* {:type :keyword} "Export an executable's symbols to native modules." nil)
(def spork/cc/*use-rpath* {:type :keyword} "Use the syspath as the runtime path of shared objects." nil)
(def spork/cc/*vcvars-cache* {:type :keyword} "Where vcvars are cached." nil)
(def spork/cc/*visit* {:type :keyword} "Callback handed each command with its inputs and outputs." nil)
(def spork/cc/ver {:type [:number]} "The running Janet's version, as numbers." nil)

(defn spork/cc/build-type
  {:params [] :ret (enum :develop :debug :release :native)}
  "The build type, `:develop` by default."
  [])
(defn spork/cc/check-library-exists
  {:params [:string :keyword? :string?] :ret :boolean}
  "Whether a test program links against the library."
  [libname &opt binding test-source-code])
(defn spork/cc/compile-and-link-executable
  {:params [:string :string] :ret @[[:string]]}
  "Compile and link an executable, answering the commands."
  [to & sources])
(defn spork/cc/compile-and-link-shared
  {:params [:string :string] :ret @[[:string]]}
  "Compile and link a shared library, answering the commands."
  [to & sources])
(defn spork/cc/compile-and-make-archive
  {:params [:string :string] :ret @[[:string]]}
  "Compile and archive a static library, answering the commands."
  [to & sources])
(defn spork/cc/compile-c
  {:params [:string :string] :ret [:string]}
  "Compile a C source to an object file, answering the command."
  [from to])
(defn spork/cc/compile-c++
  {:params [:string :string] :ret [:string]}
  "Compile a C++ source to an object file, answering the command."
  [from to])
(defn spork/cc/generic-preprocess
  {:params [:string :string] :ret [:string]}
  "Generate C from a Janet script as part of the build, answering the command."
  [from to])
(defn spork/cc/get-msvc-prefix
  {:params [] :ret :string}
  "Where Janet is installed on Windows."
  [])
(defn spork/cc/get-unix-prefix
  {:params [] :ret :string}
  "The prefix libjanet and janet.h are found under."
  [])
(defn spork/cc/link-executable-c
  {:params [(or [:string] @[:string]) :string :boolean?] :ret [:string]}
  "Link a C executable, answering the command."
  [objects to &opt make-static])
(defn spork/cc/link-executable-c++
  {:params [(or [:string] @[:string]) :string :boolean?] :ret [:string]}
  "Link a C++ executable, answering the command."
  [objects to &opt make-static])
(defn spork/cc/link-shared-c
  {:params [(or [:string] @[:string]) :string] :ret [:string]}
  "Link a C shared library, answering the command."
  [objects to])
(defn spork/cc/link-shared-c++
  {:params [(or [:string] @[:string]) :string] :ret [:string]}
  "Link a C++ shared library, answering the command."
  [objects to])
(defn spork/cc/load-settings
  {:params [(or :struct :table)] :ret :nil}
  "Set the dynamic bindings a `save-settings` snapshot holds."
  [settings])
(defn spork/cc/make-archive
  {:params [(or [:string] @[:string]) :string] :ret [:string]}
  "Make a static archive, answering the command."
  [objects to])
(defn spork/cc/msvc-compile-and-link-executable
  {:params [:string :string] :ret @[[:string]]}
  "Compile and link an executable with MSVC, answering the commands."
  [to & sources])
(defn spork/cc/msvc-compile-and-link-shared
  {:params [:string :string] :ret @[[:string]]}
  "Compile and link a shared library with MSVC, answering the commands."
  [to & sources])
(defn spork/cc/msvc-compile-and-make-archive
  {:params [:string :string] :ret @[[:string]]}
  "Compile and archive a static library with MSVC, answering the commands."
  [to & sources])
(defn spork/cc/msvc-compile-c
  {:params [:string :string] :ret [:string]}
  "Compile a C source with MSVC, answering the command."
  [from to])
(defn spork/cc/msvc-compile-c++
  {:params [:string :string] :ret [:string]}
  "Compile a C++ source with MSVC, answering the command."
  [from to])
(defn spork/cc/msvc-find
  {:params [] :ret :nil}
  "Find vcvarsall.bat and set up the environment for MSVC."
  [])
(defn spork/cc/msvc-janet-import-lib
  {:params [] :ret :string}
  "The path to the installed Janet import library."
  [])
(defn spork/cc/msvc-link-executable
  {:params [(or [:string] @[:string]) :string :boolean?] :ret [:string]}
  "Link an executable with MSVC, answering the command."
  [objects to &opt _make-static])
(defn spork/cc/msvc-link-shared
  {:params [(or [:string] @[:string]) :string] :ret [:string]}
  "Link a shared library with MSVC, answering the command."
  [objects to])
(defn spork/cc/msvc-make-archive
  {:params [(or [:string] @[:string]) :string] :ret [:string]}
  "Make a static archive with MSVC, answering the command."
  [objects to])
(defn spork/cc/msvc-setup?
  {:params [] :ret :string? :narrows :any}
  "Whether the MSVC environment is set up: the `LIBPATH` variable when it is."
  [])
(defn spork/cc/out-path
  {:params [:string :string :string?] :ret :string}
  "The output path of a source file, flattened into the build directory."
  [path to-ext &opt sep])
(defn spork/cc/pkg-config
  {:params [:string] :ret :nil}
  "Set defines, compiler and linker flags from pkg-config."
  [& pkg-config-libraries])
(defn spork/cc/save-settings
  {:params [] :ret {:keyword :any}} # TODO: each setting holds what its dynamic binding was set to
  "A snapshot of the compiler settings, for `load-settings`."
  [])
(defn spork/cc/search-dynamic-libraries
  {:params [:string] :ret @[:string]}
  "Add the dynamic libraries that exist to `*dynamic-libs*`, answering those that do not."
  [& libraries])
(defn spork/cc/search-libraries
  {:params [:string] :ret @[:string]}
  "Add the libraries that exist to `*libs*`, answering those that do not."
  [& libraries])
(defn spork/cc/search-static-libraries
  {:params [:string] :ret @[:string]}
  "Add the static libraries that exist to `*static-libs*`, answering those that do not."
  [& libraries])
(defn spork/cc/visit-add-rule
  {:params [(or [:string] @[:string])
            (or [:string] @[:string])
            (or [:string] @[:string])
            :string]
   :ret :table}
  "Add the command as a rule to `*rules*`, answering the rule."
  [cmd inputs outputs message])
(defn spork/cc/visit-clean
  {:params [(or [:string] @[:string])
            (or [:string] @[:string])
            (or [:string] @[:string])
            :string]
   :ret :nil}
  "Remove the outputs."
  [_cmd _inputs outputs _message])
(defn spork/cc/visit-do-nothing {:params [] :ret :nil} "Do nothing." [&])
(defn spork/cc/visit-execute
  {:params [(or [:string] @[:string])
            (or [:string] @[:string])
            (or [:string] @[:string])
            :string]
   :ret :number?}
  "Run the command, answering its exit code when `:verbose` is set."
  [cmd _inputs _outputs message])
(defn spork/cc/visit-execute-if-stale
  {:params [(or [:string] @[:string])
            (or [:string] @[:string])
            (or [:string] @[:string])
            :string]
   :ret :number?}
  "Run the command when an input is newer than the outputs."
  [cmd inputs outputs message])
(defn spork/cc/visit-execute-quiet
  {:params [(or [:string] @[:string])
            (or [:string] @[:string])
            (or [:string] @[:string])
            :string]
   :ret :number}
  "Run the command with its output discarded, answering its exit code."
  [cmd _inputs _outputs _message])
(defn spork/cc/visit-generate-makefile
  {:params [(or [:string] @[:string])
            (or [:string] @[:string])
            (or [:string] @[:string])
            :string]
   :ret :nil}
  "Print the command as a Makefile target."
  [cmd inputs outputs message])

# -- spork/pm ----------------------------------------------------------------------------------

(def spork/pm/*curlpath* {:type :keyword} "The curl command dependencies are fetched with." nil)
(def spork/pm/*gitpath* {:type :keyword} "The git command dependencies are fetched with." nil)
(def spork/pm/*pkglist* {:type :keyword} "The package listing, when no `pkgs` bundle is installed." nil)
(def spork/pm/*tarpath* {:type :keyword} "The tar command dependencies are unpacked with." nil)

(defn spork/pm/curl {:params [:string] :ret :number} "Run curl, answering its exit code." [& args])
(defmacro spork/pm/deftemplate
  {:params [:symbol a] :ret :function}
  "Define `template-name` as a function of a dictionary rendering the `$` template string."
  [template-name & body])
(defn spork/pm/download-bundle
  {:params [:string (enum :git :tar :file) :string?] :ret :string}
  "Fetch a bundle's source to the cache, answering where it is."
  [url bundle-type &opt tag])
(defn spork/pm/download-git-bundle
  {:params [:string :string :string?] :ret :number?}
  "Clone or update a git bundle."
  [bundle-dir url tag])
(defn spork/pm/download-tar-bundle
  {:params [:string :string] :ret :number}
  "Download and unpack a bundle from a tar archive."
  [bundle-dir url])
(defn spork/pm/git {:params [:string] :ret :number} "Run git, answering its exit code." [& args])
(defn spork/pm/jpm-dep-to-bundle-dep
  {:params [(or :string :struct :table)] :ret :string?}
  "The name of the installed bundle a jpm dependency names."
  [dep-name])
(defn spork/pm/load-lockfile
  {:params [:string] :ret :nil}
  "Install every bundle a lockfile lists."
  [lock-src])
(defn spork/pm/load-project-meta
  {:params [:string] :ret (or :struct :table)}
  "The metadata of a project, read without running project.janet."
  [dir])
(defn spork/pm/local-hook
  {:params [(or :string :symbol :keyword) a] :ret :any} # TODO: whatever the hook answers
  "Run a bundle hook of the project in the current directory."
  [hook & args])
(defn spork/pm/name-lookup
  {:params [(or :struct :table)] :ret :string?}
  "The name of the installed bundle at a bundle address."
  [bundle-addr])
(defn spork/pm/opt-ask
  {:params [:keyword (or :struct :table)] :ret :any} # TODO: the default in `input-options`, else the string typed
  "The default for `key`, or what the user types when there is none."
  [key input-options])
(defn spork/pm/pm-install
  {:params [(or :string :struct :table) :boolean? :boolean? :boolean? :boolean?] :ret :string?}
  "Fetch and install a bundle, answering its name unless it was already installed."
  [bundle-code &named no-deps force-update no-install auto-remove])
(defn spork/pm/resolve-bundle
  {:params [(or :string :struct :table)] :ret {:url :string :tag :string? :type :keyword}}
  "A bundle given by name, URL or dictionary, in its normal form."
  [bundle])
(defn spork/pm/save-lockfile
  {:params [:string]
   :ret @[{:name :string :pm (or :struct :table) :config (or :struct :table :nil)}]}
  "Write a lockfile of the installed bundles, answering its entries."
  [lock-dest])
(defn spork/pm/scaffold-pm-shell
  {:params [:string] :ret :nil}
  "Create a shell environment with its activation scripts at `path`."
  [path])
(defn spork/pm/scaffold-project
  {:params [:string (or :struct :table :nil)] :ret :nil}
  "Create a project directory from a standard template."
  [name &opt options])
(defn spork/pm/tar {:params [:string] :ret :number} "Run tar, answering its exit code." [& args])
(defn spork/pm/update-git-bundle
  {:params [:string :string?] :ret :number}
  "Fetch the tag of a git bundle and reset to it."
  [bundle-dir tag])
(defn spork/pm/vendor-binaries-pm-shell
  {:params [:string] :ret :nil}
  "Copy the Janet interpreter and its libraries into a shell environment."
  [path])
