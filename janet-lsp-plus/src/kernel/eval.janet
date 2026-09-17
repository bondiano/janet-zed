# Called through netrepl's 0xFF channel as `(this-fn code source line column)`: evaluates `code`
# in the shared REPL env (the proto of the connection's fiber env) and returns
# [value output errors]. `source` is the file the kernel found `code` in, with its 1-based `line`
# and `column` there, or :zed. With the debugger attached (`dap/driver.janet`), thunks run through
# its hook, so breakpoints stop them.
(fn [code source line column]
  (def env (or (table/getproto (curenv)) (curenv)))
  (def output @"")
  (def errors @"")
  (var value "")
  (var pending code)
  (def parser (parser/new))
  # Parser columns count from 0.
  (parser/where parser line (- column 1))
  (defn into [buf f] (fn [& args] (with-dyns [:err buf] (f ;args))))
  (defn run [thunk]
    (if-let [debugger (in env :janet-zed/debugger)]
      (let [own ((debugger :compiled) thunk)]
        (defer ((debugger :forget) own) ((debugger :run) thunk)))
      (thunk)))
  (run-context
    {:env env
     :source source
     :parser parser
     :chunks (fn [buf _] (when pending (buffer/push buf pending) (set pending nil)))
     :evaluator (fn [thunk &] (setdyn :out output) (setdyn :err output) (run thunk))
     :on-status (fn [f x]
                  (if (= :dead (fiber/status f))
                    (do (put env '_ @{:value x}) (set value (string/format "%.20q" x)))
                    ((into errors debug/stacktrace) f x "")))
     :on-compile-error (into errors bad-compile)
     :on-compile-warning (into output warn-compile)
     :on-parse-error (into errors bad-parse)})
  [value (string output) (string errors)])
