# A long-lived checker. Each line on stdin is a request, `{:file :cwd :text :includes :declared
# :packages :natives}`; each gets one line on stdout, `check/marker` and then, as JSON,
# `{:problems [...] :bindings {path [[name line col doc private types] ...]}}`, or `error` and the
# message as a JSON string. `:bindings` are the names macros bound, which the host cannot read
# from the source: in `:file`, and in each module loaded since the previous request. Like core `flycheck`,
# every form of the file is parsed and compiled and macros are expanded, but only forms known to
# be safe run: definitions without side effects, imports, and anything with `:flycheck` metadata.
# janet-zed: include ./json.janet ./project.janet ./types.janet
# janet-zed: declare script/root-env check/vocabulary

# Checked code and imported modules may write to stdout too.
(def- check/marker "\x01janet-zed ")
(var- check/file nil)
(def- check/problems @[])

(defn- check/report [severity message line col]
  (array/push check/problems @{:severity severity :message message :line line :col col}))

# `no-side-effects` and `is-safe-def` from boot.janet, where they are private. Unlike core,
# quoted data such as `{'GET :get}` counts as pure.
(defn- check/pure? [src]
  (cond
    (and (tuple? src) (= :parens (tuple/type src)) (= 'quote (first src))) true
    (tuple? src) (if (= (tuple/type src) :brackets) (all check/pure? src))
    (array? src) (all check/pure? src)
    (dictionary? src) (and (all check/pure? (keys src)) (all check/pure? (values src)))
    true))

(defn- check/safe-def [thunk source env where]
  (if-let [binding (get env (source 1))
           flycheck (get binding :flycheck)]
    (if (function? flycheck) (flycheck thunk source env where) (thunk))
    (if (check/pure? (last source)) (thunk))))

(def- check/importers {'import true 'import* true 'use true 'require true 'dofile true})
# Unlike core `flycheck`, imported modules load as Janet loads them, fully run: names a module
# binds at run time (a re-export loop, a table a call builds) exist for its importers. Only the
# checked file itself follows the flycheck rules.
# `import` and `use` name their module literally; the functions load only a path the form spells
# out, not whatever a skipped definition left in a variable.
(defn- check/literal-import [thunk source &]
  (when (string? (get source 1)) (thunk)))
(def- check/specials
  (merge check/importers
         {'import* check/literal-import 'require check/literal-import 'dofile check/literal-import
          # Core runs a top-level `assert` while flychecking, but its result is never reported,
          # and a test's assertions have side effects: written snapshots, started servers.
          'assert false}
         (tabseq [name :in '[def var def- var- defglobal varglobal]] name check/safe-def)))
(def- check/definers
  (tabseq [name :in '[def def- var var- defn defn- defmacro defmacro- varfn defdyn
                      defglobal varglobal]]
    name true))

# Where the top-level definitions of `text` start, `[line col]`: the host reads those itself.
(defn- check/definition-starts [text]
  (def starts @{})
  (each form (or (try (parse-all text) ([_] nil)) [])
    (when (and (tuple? form) (check/definers (first form)))
      (put starts (tuple/slice (tuple/sourcemap form)) true)))
  starts)

# The names `env` binds from other forms of the file at `path`, with the types their metadata
# declares: the compiler records those when it compiles the `def` a macro expanded to.
(defn- check/bindings [env path text]
  (def starts (check/definition-starts text))
  (seq [[name binding] :pairs env
        :when (and (symbol? name) (table? binding))
        :let [[at line col] (or (get binding :source-map) [])]
        :when (and (= at path) (not (starts [line col])))]
    [(string name) line col (get binding :doc) (truthy? (get binding :private))
     (types/declared binding)]))

# The top-level form compiling or running now: a failure is reported at it.
(var- check/form nil)

# A definition that fails to compile still names something: without a stand-in, every later
# use of the name would be another `unknown symbol`.
(defn- check/stand-in [env]
  (when (and (tuple? check/form)
             (check/definers (get check/form 0))
             (symbol? (get check/form 1)))
    (put env (check/form 1) @{:value nil})))

(defn- check/evaluator [thunk source env where]
  (when (and (tuple? source) (= (tuple/type source) :parens))
    (def head (source 0))
    (def flycheck (get check/specials head (get (get env head {}) :flycheck)))
    (cond
      (function? flycheck) (flycheck thunk source env where)
      flycheck (thunk))))

# What imported modules load into. Their output must not corrupt the replies.
(def- check/base (make-env script/root-env))
(put check/base :flychecking true)
(put check/base :out @"")
(put check/base :err @"")
(put check/base *module-make-env* (fn [&] (make-env check/base)))

# -- where modules are found -----------------------------------------------------------------

# From the request: `[module path]` pairs for the workspace's `declare-source` modules (`path` is
# a `.janet` file or a directory of modules) and its `declare-native` ones (`path` without the
# extension); and the project's `jpm_tree/lib`, when it has one.
(var- check/packages [])
(var- check/natives [])
(var- check/tree nil)

# `.so`, or `.dll` on Windows.
(def- check/native-extension (string (module/expand-path "" ":native:")))

(defn- check/existing [& files]
  (find |(= :file (os/stat $ :mode)) files))

# Workspace sources come ahead of installed copies: a monorepo package imports its siblings.
(defn- check/package-file [spec]
  (some (fn [[module path]]
          (cond
            (= spec module) (check/existing path (string path "/init.janet"))
            (string/has-prefix? (string module "/") spec)
            (let [base (string path (string/slice spec (length module)))]
              (check/existing (string base ".janet") (string base "/init.janet")))))
        check/packages))

(defn- check/native-file [spec]
  (some (fn [[module path]]
          (and (= spec module) (check/existing (string path check/native-extension))))
        check/natives))

# Dependencies installed with `jpm -l`.
(defn- check/in-tree [& suffixes]
  (fn [spec]
    (when (and check/tree (not (some |(string/has-prefix? $ spec) ["." "/" "@"])))
      (check/existing ;(map |(string check/tree "/" spec $) suffixes)))))

(array/insert module/paths 0 [(check/in-tree ".janet" "/init.janet") :source])
(array/insert module/paths 0 [(check/in-tree check/native-extension) :native])
(array/insert module/paths 0 [check/package-file :source])
(array/insert module/paths 0 [check/native-file :native])

# -- the module cache ------------------------------------------------------------------------

# Imported modules stay in `module/cache` between requests, so their top-level code runs once. A
# module whose file changed is unloaded together with every module that imported it: they hold
# its old bindings.
(def- check/importers-of @{})
(def- check/fingerprints @{})
(var- check/finding false)

# `require` asks `module/find` even for a cached module: this template records who imports what
# and finds nothing itself.
(defn- check/note-import [spec]
  (def from (dyn :current-file))
  (unless (or check/finding (nil? from))
    (set check/finding true)
    (def [path] (defer (set check/finding false) (module/find spec)))
    (when path (put-in check/importers-of [path from] true)))
  nil)
(array/insert module/paths 0 [check/note-import :source])

# The modification time and size of the file, read without reading the file. Janet's times are
# whole seconds, so a file modified within the last one could change again without them showing
# it: its contents count too, until the next request finds it settled (and reloads it once).
# ponytail: Janet's 32-bit `hash` of those contents; a collision keeps a stale module until the
# file changes again.
(defn- check/fingerprint [path]
  (when-let [stat (os/stat path)]
    (def modified (stat :modified))
    [modified (stat :size)
     (when (>= modified (dec (os/time)))
       (hash (try (string (slurp path)) ([_] nil))))]))

(defn- check/unload [path]
  (put check/fingerprints path nil)
  (when (in module/cache path)
    (put module/cache path nil)
    (eachk importer (get check/importers-of path {})
      (check/unload importer))))

(defn- check/unload-changed []
  (each path (keys check/fingerprints)
    (unless (= (check/fingerprints path) (check/fingerprint path))
      (check/unload path))))

# Modules loaded since the last request, and what their macros bound into `bindings`.
(defn- check/remember-loaded [bindings]
  (eachk path module/cache
    (when (and (string? path)
               (string/has-suffix? ".janet" path)
               (nil? (check/fingerprints path)))
      (put check/fingerprints path (check/fingerprint path))
      (when-let [text (try (string (slurp path)) ([_] nil))]
        (put bindings path (check/bindings (module/cache path) path text))))))

(defn- check/clear [table]
  (each key (keys table) (put table key nil)))

# -- checking --------------------------------------------------------------------------------

(defn- check/run [request]
  (def {:file file :cwd cwd :text text :includes includes :declared declared
        :packages packages :natives natives} request)
  (os/cd cwd)
  (set check/tree (if (os/stat "jpm_tree/lib") (string cwd "/jpm_tree/lib")))
  # Modules resolved against other workspace modules may hold the wrong imports.
  (unless (= [packages natives] [check/packages check/natives])
    (check/clear module/cache)
    (check/clear check/fingerprints))
  (set check/packages packages)
  (set check/natives natives)
  (check/unload-changed)
  (set check/file file)
  (set check/form nil)
  (array/clear check/problems)
  (buffer/clear (check/base :out))
  (buffer/clear (check/base :err))

  (def env (make-env check/base))
  # `# janet-zed: declare` names get stand-ins; `# janet-zed: include` files load into this env,
  # as when the host concatenates them. A broken include is not this file's problem.
  # A declaration of a core name, like `core.d.janet`, must not shadow the real binding.
  (each name declared
    (unless (get env (symbol name))
      (put env (symbol name) @{:value nil})))
  (each path includes
    (protect (dofile path :env env :evaluator check/evaluator)))
  (put env :current-file file)

  # project.janet's vocabulary: the bindings of the installed jpm and janet-pm, over stand-ins for
  # `check/vocabulary` (defined by the host) when neither is installed.
  (when (string/has-suffix? "project.janet" file)
    (each name check/vocabulary
      (put env name @{:value (fn [& _] nil)}))
    # `post-deps` would load dependencies while expanding.
    (put env :jpm-no-deps true)
    (each project-env (project/envs)
      (eachp [name binding] project-env
        (when (symbol? name)
          (put env name binding)))))

  (var pending text)
  (run-context
    {:env env
     :source file
     :chunks (fn [buf _]
               (when pending
                 (buffer/push buf pending "\n")
                 (set pending nil)))
     :expander (fn [source] (set check/form source))
     :evaluator check/evaluator
     :on-compile-error (fn [message _ where &opt line col]
                         (when (= where file)
                           (check/report 1 message line col)
                           (check/stand-in env)))
     :on-compile-warning (fn [message _ where &opt line col]
                           (when (= where file) (check/report 2 message line col)))
     :on-parse-error (fn [parser where]
                       (def [line col] (parser/where parser))
                       (check/report 1 (parser/error parser) line col))
     # Only a failing import is worth reporting: other forms run on a partial environment
     # (skipped definitions), so their runtime errors are mostly noise.
     :on-status (fn [fiber value]
                  (when (and (not= :dead (fiber/status fiber))
                             check/form
                             (check/importers (check/form 0)))
                    (def [line col] (tuple/sourcemap check/form))
                    (check/report 1 (string value) line col)))})
  (def bindings @{file (check/bindings env file text)})
  (check/remember-loaded bindings)
  {:problems check/problems :bindings bindings})

(loop [line :iterate (file/read stdin :line)]
  (def reply
    (try
      (string (json/encode (check/run (parse line))))
      ([err] (string "error " (json/encode (string err))))))
  (print check/marker reply)
  (flush))
