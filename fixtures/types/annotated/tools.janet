# spork in use with written types around it: JSON in and out, paths, argument parsing and
# running commands.

(import spork/json)
(import spork/path)
(import spork/argparse)
(import spork/sh)

(def Settings :typedef {:name :string :verbose :boolean :files @[:string]})

(defn settings
  {:params [[:string]] :ret Settings?}
  "The settings the command line asks for, or nil when it does not parse."
  [args]
  (def parsed
    (with-dyns [:args @["tool" ;args]
                :out @""]
      (argparse/argparse
        "An annotated tool."
        "name" {:kind :option :short "n" :default "world" :help "Who to greet."}
        "verbose" {:kind :flag :short "v" :help "Say more."}
        :default {:kind :accumulate})))
  (when parsed
    {:name (parsed "name")
     :verbose (truthy? (parsed "verbose"))
     :files (or (parsed :default) @[])}))

(defn to-json
  {:params [Settings] :ret :string}
  "Settings as a JSON document."
  [s]
  (string (json/encode s)))

(defn from-json
  {:params [(or :string :buffer)] :ret {:string :any}}
  "A JSON object, keys kept as strings."
  [text]
  (json/decode text))

(defn stems
  {:params [[:string]] :ret @[:string]}
  "Each path's file name without its extension."
  [paths]
  (map (fn [p]
         (def base (path/basename p))
         (if-let [ext (path/ext base)]
           (string/slice base 0 (- (length base) (length ext)))
           base))
       paths))

(defn under
  {:params [:string :string] :ret :string}
  "`file` joined under `dir`, normalized."
  [dir file]
  (path/normalize (path/join dir file)))

(defn janet-says
  {:params [:string] :ret :string}
  "What this Janet prints for the code."
  [code]
  (sh/exec-slurp (dyn *executable*) "-e" code))

(defn words
  {:params [(or :string :buffer)] :ret @[:string]}
  "A command line split the way a shell would, or nothing where it does not parse."
  [line]
  (or (sh/split line) @[]))

(def s (settings ["-n" "ann" "-v" "a.txt" "b/c.janet"]))
(assert s)
(assert (= "ann" (s :name)))
(assert (s :verbose))
(assert (deep= @["a" "c"] (stems (s :files))))
(assert (nil? (settings ["--no-such-flag"])))

(def decoded (from-json (to-json s)))
(assert (= "ann" (decoded "name")))
(assert (= 2 (length (decoded "files"))))
(def round (from-json @`{"n": 1}`))
(assert (= 1 (round "n")))

(assert (= "a/c.txt" (under "a/b/.." "c.txt")))
(assert (= "3" (janet-says "(prin (+ 1 2))")))
(assert (deep= @["a" "b c"] (words `a "b c"`)))
(assert (deep= @["x"] (words @"x")))
