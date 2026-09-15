# Checks the source on stdin as the file `check/file` (defined by the caller). Like core
# `flycheck`, every form is parsed and compiled and macros are expanded, but only forms known
# to be safe run: definitions without side effects, imports, and anything with `:flycheck`
# metadata. Prints the problems found in this file as one JSON array.
# janet-zed: include ./json.janet ./project.janet
# janet-zed: declare script/root-env check/file check/vocabulary check/includes check/declared

(def- check/problems @[])

(defn- check/report [severity message line col]
  (array/push check/problems @{:severity severity :message message :line line :col col}))

# `no-side-effects` and `is-safe-def` from boot.janet, where they are private.
(defn- check/pure? [src]
  (cond
    (tuple? src) (if (= (tuple/type src) :brackets) (all check/pure? src))
    (array? src) (all check/pure? src)
    (dictionary? src) (and (all check/pure? (keys src)) (all check/pure? (values src)))
    true))

(defn- check/safe-def [thunk source env where]
  (if-let [binding (get env (source 1))
           flycheck (get binding :flycheck)]
    (if (function? flycheck) (flycheck thunk source env where) (thunk))
    (if (check/pure? (last source)) (thunk))))

(var- check/evaluator nil)

(defn- check/use [thunk source env where]
  (each module (drop 1 source)
    (import* (string module) :prefix "" :evaluator check/evaluator)))

(def- check/specials @{'def check/safe-def 'var check/safe-def 'use check/use})
(def- check/importers {'import true 'import* true 'use true 'require true 'dofile true})
(def- check/definers
  (tabseq [name :in '[def def- var var- defn defn- defmacro defmacro- varfn defdyn
                      defglobal varglobal]]
    name true))

# The top-level form compiling or running now: a failure is reported at it.
(var- check/form nil)

# A definition that fails to compile still names something: without a stand-in, every later
# use of the name would be another `unknown symbol`.
(defn- check/stand-in [env]
  (when (and (tuple? check/form)
             (check/definers (get check/form 0))
             (symbol? (get check/form 1)))
    (put env (check/form 1) @{:value nil})))

(set check/evaluator
     (fn [thunk source env where]
       (when (and (tuple? source) (= (tuple/type source) :parens))
         (def head (source 0))
         (def flycheck (get check/specials head (get (get env head {}) :flycheck)))
         (cond
           (function? flycheck) (flycheck thunk source env where)
           flycheck (thunk)))))

(def- check/env (make-env script/root-env))
(put check/env :flychecking true)
# Output from checked code (macros, safe definitions, imports) must not corrupt the result.
(put check/env :out @"")
(put check/env :err @"")
(put check/env *module-make-env* (fn [&] (make-env check/env)))

# `# janet-zed: declare` names get stand-ins; `# janet-zed: include` files load into this env,
# as when the host concatenates them. A broken include is not this file's problem.
(each name check/declared
  (put check/env (symbol name) @{:value nil}))
(each path check/includes
  (protect (dofile path :env check/env :evaluator check/evaluator)))
(put check/env :current-file check/file)

# Dependencies installed with `jpm -l`.
(when (os/stat "jpm_tree/lib")
  (module/add-syspath "jpm_tree/lib"))

# project.janet's vocabulary: the bindings of the installed jpm and janet-pm, over stand-ins for
# `check/vocabulary` (defined by the caller) when neither is installed.
(when (string/has-suffix? "project.janet" check/file)
  (each name check/vocabulary
    (put check/env name @{:value (fn [& _] nil)}))
  # `post-deps` would load dependencies while expanding.
  (put check/env :jpm-no-deps true)
  (each env (project/envs)
    (eachp [name binding] env
      (when (symbol? name)
        (put check/env name binding)))))

(var- check/pending (file/read stdin :all))

(run-context
  {:env check/env
   :source check/file
   :chunks (fn [buf _]
             (when check/pending
               (buffer/push buf check/pending "\n")
               (set check/pending nil)))
   :expander (fn [source] (set check/form source))
   :evaluator check/evaluator
   :on-compile-error (fn [message _ where &opt line col]
                       (when (= where check/file)
                         (check/report 1 message line col)
                         (check/stand-in check/env)))
   :on-compile-warning (fn [message _ where &opt line col]
                         (when (= where check/file) (check/report 2 message line col)))
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

(print (json/encode check/problems))
