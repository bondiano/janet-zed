# The debuggee side of `janet-lsp-plus dap`. It connects back to the adapter and reads one
# Janet form per line: `[id name & args]`. It writes JSON lines: `{"id": id, "body": …}` (or
# `"error"`) answers a command, `{"event": …}` reports a stop or a verified breakpoint.
#
# Launch mode runs a program like `janet program args`. Attach mode (`driver/attach`) runs inside
# the REPL's netrepl process and puts `driver/run` into the shared env for the kernel's eval.
# janet-zed: include ../janet/json.janet

(var- driver/conn nil)
(var- driver/mode :launch)
# Commands for the fiber serving a stop; breakpoints are handled as they arrive.
(def- driver/commands (ev/chan 1024))
# The env evaluations fall back to: the program's, or the REPL's shared one.
(var- driver/env nil)
(var- driver/uncaught true)
# Stop at the start of the next thunk, with this reason ("entry" or "step").
(var- driver/at-start nil)

# Breakpoints. Paths are real paths; `debug/break` gets the source string funcdefs carry.
(def- driver/wanted @{}) # path -> line -> breakpoint id
(def- driver/applied @{}) # path -> line -> [source column]
(def- driver/lines @{}) # path -> line -> [source column] of the line's first instruction
(def- driver/modules @{}) # module/cache key -> env already scanned
(def- driver/realpaths @{})

# The stop being served: its frames, and values expanded through variablesReference ids.
(var- driver/stack @[])
(def- driver/refs @[])

(defn- driver/send [x]
  # The adapter may be gone already.
  (protect (net/write driver/conn (string (json/encode x) "\n"))))

(defn- driver/real [source]
  (unless (driver/realpaths source)
    (put driver/realpaths source (try (os/realpath source) ([_] source))))
  (driver/realpaths source))

(defn- driver/table-in [t k]
  (or (in t k) (do (put t k @{}) (in t k))))

### Breakpoints

(defn- driver/scan
  "Records the first instruction of each line of funcdef disassembly `d`, unless an earlier scan
  of the same batch (`fresh`: path -> line -> true) claimed the line. Returns the lines claimed."
  [d fresh]
  (def source (d :source))
  (def claimed @[])
  (when (and (string? source) (d :sourcemap))
    (def path (driver/real source))
    (def lines (driver/table-in driver/lines path))
    (def seen (driver/table-in fresh path))
    (each [line column] (d :sourcemap)
      (when (and (pos? line) (not (seen line)))
        (put seen line true)
        (put lines line [source column])
        (array/push claimed [path line]))))
  claimed)

(defn- driver/scan-tree [d fresh]
  (each sub (or (d :defs) []) (driver/scan-tree sub fresh))
  (driver/scan d fresh))

(defn- driver/scan-env
  "Scans the functions bound in `env`."
  [env fresh]
  (eachp [_ binding] env
    (when (table? binding)
      (def value (or (binding :value) (get (binding :ref) 0)))
      (when (function? value)
        (driver/scan-tree (disasm value) fresh)))))

(defn- driver/scan-modules
  "Scans the functions of modules loaded since the last call: their top-level code already ran."
  [fresh]
  (eachp [key env] module/cache
    (when (and (table? env) (not= env (driver/modules key)))
      (put driver/modules key env)
      (driver/scan-env env fresh))))

(defn- driver/break [path line id]
  (def [source column] (get-in driver/lines [path line]))
  (when (protect (debug/break source line column))
    (def applied (driver/table-in driver/applied path))
    (unless (applied line)
      (driver/send {:event "breakpoint" :id id :line line :verified true}))
    (put applied line [source column])))

(defn- driver/apply
  "Sets the wanted breakpoints on the lines `fresh` holds: newly compiled code gets them even
  where an older definition had them."
  [fresh]
  (eachp [path seen] fresh
    (eachp [line id] (or (driver/wanted path) {})
      (when (seen line) (driver/break path line id)))))

(defn- driver/set-breakpoints
  "Replaces the breakpoints of `path` with `pairs` of [id line]."
  [path pairs]
  (def path (driver/real path))
  (eachp [line [source column]] (or (driver/applied path) {})
    (protect (debug/unbreak source line column)))
  (put driver/applied path @{})
  (def wanted @{})
  (each [id line] pairs (put wanted line id))
  (put driver/wanted path wanted)
  (eachp [line id] wanted
    (when (get-in driver/lines [path line])
      (driver/break path line id))))

(defn- driver/rebreak
  "Sets again the breakpoints on `line`:`column`: `debug/unfbreak` clears any breakpoint of its
  instruction."
  [line column]
  (eachp [_ applied] driver/applied
    (when-let [[source c] (applied line)]
      (when (= c column) (protect (debug/break source line column))))))

(defn driver/compiled
  "Prepares breakpoints for `thunk`, a compiled top-level form. Returns the lines only `thunk`
  itself holds, to forget once it has run."
  [thunk]
  # Lets the command reader take breakpoints that came while the program ran.
  (ev/sleep 0)
  (def fresh @{})
  (driver/scan-modules fresh)
  (def d (disasm thunk))
  # Nested functions first: a line shared with the top-level form breaks in the function.
  (each sub (or (d :defs) []) (driver/scan-tree sub fresh))
  (def own (driver/scan d fresh))
  (driver/apply fresh)
  own)

(defn driver/forget [own]
  (each [path line] own
    (put (driver/lines path) line nil)
    (when-let [applied (driver/applied path)] (put applied line nil))))

### Inspecting a stop

(defn- driver/frames
  "Frames of `f` and the child fibers it is stopped in, innermost first."
  [f]
  (mapcat debug/stack (reverse (debug/lineage f))))

(defn- driver/show [value depth]
  (def text (string/format (string "%." depth "q") value))
  (if (> (length text) 4096) (string (string/slice text 0 4096) "…") text))

(defn- driver/ref [value]
  (if (and (index-of (type value) [:table :struct :array :tuple]) (not (empty? value)))
    (length (array/push driver/refs value))
    0))

(defn- driver/variable [name value]
  {:name name
   :value (driver/show value 2)
   :type (string (type value))
   :variablesReference (driver/ref value)})

(defn- driver/children [value]
  (if (dictionary? value)
    (map |(driver/variable (if (symbol? $) (string $) (string/format "%q" $)) (in value $))
         (take 1000 (sorted (keys value))))
    (seq [i :range [0 (min 1000 (length value))]]
      (driver/variable (string i) (in value i)))))

(defn- driver/frame [id frame]
  (def source (frame :source))
  (merge
    {:id id
     :name (or (frame :name) "<anonymous>")
     :line (max 0 (or (frame :source-line) 0))
     :column (max 0 (or (frame :source-column) 0))}
    (if (and (string? source) (os/stat source))
      {:source {:path (driver/real source)}}
      {:presentationHint "subtle"})))

(defn- driver/evaluate
  "Evaluates `code` with the locals of `frame` over its module's env. Writes to locals stay in the
  evaluation."
  [frame code]
  (def base (or (module/cache (get frame :source)) driver/env))
  (def env (table/setproto @{} base))
  (eachp [name value] (or (get frame :locals) {})
    (put env name @{:value value}))
  (def f (fiber/new (fn []
                      (var result nil)
                      (each form (parse-all code) (set result (eval form env)))
                      result)
                    :ed))
  (def result (resume f))
  (case (fiber/status f)
    :dead {:result (driver/show result 20) :variablesReference (driver/ref result)}
    :debug (error "the evaluation hit a breakpoint")
    (error (string result))))

(defn- driver/answer [name args]
  (case name
    :stackTrace {:stackFrames (seq [[id frame] :pairs driver/stack] (driver/frame id frame))
                 :totalFrames (length driver/stack)}
    :scopes {:scopes (if-let [locals (get-in driver/stack [(args 0) :locals])]
                       [{:name "Locals"
                         :presentationHint "locals"
                         :variablesReference (driver/ref locals)
                         :expensive false}]
                       [])}
    :variables {:variables (driver/children (driver/refs (- (args 0) 1)))}
    :evaluate (driver/evaluate (get driver/stack (or (args 0) 0)) (args 1))
    (error (string "unknown command " name))))

### Running and stepping

(defn- driver/exit [code]
  (file/flush stdout)
  (file/flush stderr)
  (os/exit code))

(defn- driver/position [f]
  (def frames (driver/frames f))
  [(length frames) (get-in frames [0 :source-line])])

(defn- driver/resume-to
  "Resumes `f` with a temporary breakpoint at `pc` of `fun`. Returns [value arrived]: `arrived`
  is false when `f` stopped somewhere else."
  [f fun pc]
  (def [line column] (get (disasm fun :sourcemap) pc))
  (debug/fbreak fun pc)
  (def value (defer (do (debug/unfbreak fun pc) (driver/rebreak line column))
               (resume f)))
  (def top (first (driver/frames f)))
  [value (or (not= :debug (fiber/status f))
             (and (= pc (top :pc)) (= line (top :source-line)) (= column (top :source-column))))])

(defn- driver/step-instruction
  "Runs one instruction of the innermost fiber. `debug/step` runs a whole call and runs away on a
  return, so calls (for step in) and returns go to temporary breakpoints."
  [f kind]
  (def lineage (debug/lineage f))
  (def fiber (last lineage))
  (def stack (debug/stack fiber))
  (def top (first stack))
  (def depth (length (driver/frames f)))
  (def [op a b] (or (get (disasm (top :function) :bytecode) (top :pc)) []))
  (defn stepped [target]
    (def value (debug/step target))
    [value (or (not= :debug (fiber/status f)) (<= (length (driver/frames f)) depth))])
  (defn finished []
    (def value (resume f))
    [value (not= :debug (fiber/status f))])
  (def callee (case op 'call (get-in top [:slots b]) 'tcall (get-in top [:slots a])))
  (cond
    (and (= kind :stepIn) (function? callee)) (driver/resume-to f callee 0)
    (not (index-of op ['ret 'retn 'tcall])) (stepped fiber)
    (if-let [caller (find |($ :function) (slice stack 1))]
      (driver/resume-to f (caller :function) (+ 1 (caller :pc)))
      (if (one? (length lineage))
        (finished)
        # The parent sits on the `resume` of this fiber: its step lets the return through.
        (stepped (lineage (- (length lineage) 2)))))))

(defn- driver/step
  "Steps `f` by lines: `:next` until the line changes at the same or a shallower depth, `:stepIn`
  until the line or the depth changes, `:stepOut` until the depth drops. Returns [value reason]."
  [f kind]
  (def [depth line] (driver/position f))
  (var result nil)
  (while (nil? result)
    (def [value arrived] (driver/step-instruction f kind))
    (def [d l] (if (= :debug (fiber/status f)) (driver/position f) [0 nil]))
    (cond
      (not= :debug (fiber/status f))
      (do
        # A launched program goes on to its next top-level form; stop there.
        (when (and (= driver/mode :launch) (= :dead (fiber/status f)))
          (set driver/at-start "step"))
        (set result [value nil]))
      (not arrived) (set result [value "breakpoint"])
      (case kind
        :stepOut (< d depth)
        :next (or (< d depth) (and (= d depth) (not= l line)))
        (or (not= d depth) (not= l line)))
      (set result [value "step"])))
  result)

(defn- driver/disconnected
  "The adapter went away: a launched program ends, a REPL evaluation goes on without breakpoints."
  [f]
  (when (= driver/mode :launch) (os/exit 0))
  (if (= :debug (fiber/status f)) [(resume f) "breakpoint"] [nil nil]))

(defn- driver/serve
  "Answers commands about `f`, stopped for `reason`, until one resumes it. Returns [value reason]
  of that resume."
  [f reason text]
  (def fresh @{})
  (driver/scan-modules fresh)
  (driver/apply fresh)
  (set driver/stack (driver/frames f))
  (file/flush stdout)
  (file/flush stderr)
  (driver/send (merge {:event "stopped" :reason reason} (if text {:text text} {})))
  (var result nil)
  (while (nil? result)
    (def command (ev/take driver/commands))
    (if (nil? command)
      (set result (driver/disconnected f))
      (let [[id name & args] command]
        (if (index-of name [:continue :next :stepIn :stepOut])
          (do
            (driver/send {:id id :body {}})
            (array/clear driver/refs)
            (set driver/stack @[])
            (set result
                 (cond
                   (not= :debug (fiber/status f)) [nil nil]
                   (= name :continue) [(resume f) "breakpoint"]
                   (driver/step f name))))
          (try
            (driver/send {:id id :body (driver/answer name args)})
            ([err] (driver/send {:id id :error (string err)})))))))
  result)

(defn- driver/trace [f err]
  (def buf @"")
  (with-dyns [:err buf] (debug/stacktrace f err ""))
  (string buf))

(defn driver/run
  "Runs `thunk` in a debug fiber: breakpoints stop it, and so do errors while `uncaught` is on.
  `entry` is the function a pending stop at the start breaks in. Errors propagate after the stop."
  [thunk &opt entry]
  (def launch (= driver/mode :launch))
  (def f (fiber/new thunk (if (or driver/uncaught launch) :dei :di)))
  (var outcome
    (if-let [reason driver/at-start]
      (do
        (set driver/at-start nil)
        (def [value arrived] (driver/resume-to f (or entry thunk) 0))
        [value (if arrived reason "breakpoint")])
      [(resume f) "breakpoint"]))
  (while (= :debug (fiber/status f))
    (set outcome (driver/serve f (outcome 1) nil)))
  (def value (outcome 0))
  (when (= :error (fiber/status f))
    (when driver/uncaught
      (driver/serve f "exception" (driver/trace f value)))
    # A launched program ends as `janet` ends it, with the trace of its own frames only.
    (when launch
      (debug/stacktrace f value "")
      (driver/exit 1))
    (propagate value f))
  value)

### Connection

(defn- driver/read-commands []
  (def buf @"")
  (while (try (net/read driver/conn 65536 buf) ([_] nil))
    (while (def newline (string/find "\n" buf))
      (def command (parse (buffer/slice buf 0 newline)))
      (def [_ name & args] command)
      (buffer/blit buf buf 0 (+ newline 1))
      (buffer/popn buf (+ newline 1))
      (case name
        :breakpoints (driver/set-breakpoints ;args)
        :exceptions (set driver/uncaught (args 0))
        (ev/give driver/commands command))))
  (ev/chan-close driver/commands))

(defn- driver/fail [f x]
  (unless (= :dead (fiber/status f))
    (debug/stacktrace f x "")
    (driver/exit 1)))

(defn driver/launch
  "Connects to the adapter on `port`, waits for `[id :launch program args stop-on-entry]` and runs
  the program as `janet` runs a file, `main` included."
  [port]
  (set driver/conn (net/connect "127.0.0.1" port))
  (def reader (ev/spawn (driver/read-commands)))
  (def [_ _ program args stop-on-entry] (ev/take driver/commands))
  (def path (driver/real program))
  (def env (make-env root-env))
  (def subargs @[program ;args])
  (put env :args subargs)
  (put env :current-file path)
  (put env :source path)
  (set driver/env env)
  (when stop-on-entry (set driver/at-start "entry"))
  (def file (file/open path :rb))
  (unless file
    (eprint "could not find file " program)
    (driver/exit 1))
  (run-context
    {:env env
     :source path
     :chunks (fn [buf _] (:read file 4096 buf))
     :evaluator (fn [thunk &]
                  (def own (driver/compiled thunk))
                  (defer (driver/forget own) (driver/run thunk)))
     :on-status driver/fail
     :on-compile-error (fn [& args] (bad-compile ;args) (driver/exit 1))
     :on-parse-error (fn [& args] (bad-parse ;args) (driver/exit 1))})
  (:close file)
  (when-let [entry (in env 'main)
             main (or (get entry :value) (get (get entry :ref) 0))]
    (def f (fiber/new (fn [] (driver/run (fn [] (main ;subargs)) main)) :e))
    (fiber/setenv f env)
    (driver/fail f (resume f)))
  # The program's own ev tasks keep the process alive, as with `janet`.
  (net/close driver/conn)
  (file/flush stdout))

(defn- driver/detach
  "Removes the hook and every breakpoint: the REPL goes on as before attaching."
  [env]
  (put env :janet-zed/debugger nil)
  (set driver/uncaught false)
  (eachp [_ applied] driver/applied
    (eachp [line [source column]] applied
      (protect (debug/unbreak source line column))))
  (table/clear driver/applied)
  (table/clear driver/wanted))

(defn driver/attach
  "Connects to the adapter on `port` from inside the REPL process and puts the hook the kernel's
  eval.janet runs thunks through into the shared env. A task serves the connection until the
  adapter goes away."
  [port]
  (def env (or (table/getproto (curenv)) (curenv)))
  (set driver/mode :attach)
  (set driver/uncaught false)
  (set driver/env env)
  (set driver/conn (net/connect "127.0.0.1" port))
  # Functions defined before attaching get breakpoints too.
  (driver/scan-env env @{})
  (put env :janet-zed/debugger
       {:compiled driver/compiled :forget driver/forget :run driver/run})
  (ev/spawn
    (driver/read-commands)
    (driver/detach env))
  # netrepl replies in JDN, which has no fibers: a fiber result closes the connection.
  nil)
