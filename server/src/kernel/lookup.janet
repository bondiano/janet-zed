# Called through netrepl's 0xFF channel as `(this-fn candidates)`, `[[path name] ...]`: the first
# name the REPL binds, as the module loaded from `path` does, or, when that module is not loaded,
# as the shared REPL env does (names defined interactively). Returns strings, "" for unknown:
# [cwd source line column doc type macro], or [] when no candidate is bound. `cwd` is there
# because `source` may be relative to it.
(fn [candidates]
  (def repl (or (table/getproto (curenv)) (curenv)))
  (defn real [file] (try (os/realpath file) ([_] nil)))
  (defn loaded [path]
    (some (fn [[key env]] (when (and (string? key) (= path (real key))) env))
          (pairs module/cache)))
  (defn text [x] (if (nil? x) "" (string x)))
  (or (some (fn [[path name]]
              (def binding (table/rawget (or (loaded path) repl) (symbol name)))
              (when (table? binding)
                (def [source line column] (or (binding :source-map) []))
                (def value (if-let [ref (binding :ref)] (ref 0) (binding :value)))
                [(os/cwd) (text source) (text line) (text column) (text (binding :doc))
                 (string (type value)) (if (binding :macro) "macro" "")]))
            candidates)
      []))
