# Types of spork's `json`, `http`, `path`, `sh` and `misc`, from spork 1.10.0.
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
