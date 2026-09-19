# The debuggee side of `janet-lsp-plus dap`. It connects back to the adapter and reads one
# Janet form per line: `[id name & args]`. It writes JSON lines: `{"id": id, "body": …}` (or
# `"error"`) answers a command, `{"event": …}` reports a stop or a verified breakpoint.
#
# Launch mode runs a program like `janet program args`. Attach mode (`driver/attach`) runs inside
# the REPL's netrepl process and puts `driver/run` into the shared env for the kernel's eval.
# janet-zed: include ../../../janet-check/src/janet/json.janet

(var- driver/conn nil)
(var- driver/mode :launch)
# Commands for the fiber serving a stop; breakpoints are handled as they arrive.
(def- driver/commands (ev/chan 1024))
# The env evaluations fall back to: the program's, or the REPL's shared one.
(var- driver/env nil)
(var- driver/uncaught true)
# Stop at the start of the next thunk, with this reason ("entry" or "step").
(var- driver/at-start nil)
# The adapter pauses with SIGUSR1, whose handler sets this and interrupts the VM: debug fibers
# catch the interrupt and stop. SIGINT interrupts too, for the REPL kernel to cancel an evaluation.
(var- driver/pausing false)

# Breakpoints. Paths are real paths; `debug/break` gets the source string funcdefs carry.
(def- driver/wanted @{}) # path -> line -> {:id :condition :log}
(def- driver/applied @{}) # path -> line -> [source column]
(def- driver/lines @{}) # path -> line -> [source column] of the line's first instruction
(def- driver/modules @{}) # module/cache key -> env already scanned
(def- driver/realpaths @{})

# The stop being served: its frames, and values expanded through variablesReference ids.
(var- driver/stack @[])
(def- driver/refs @[])

# Debug fibers as DAP threads: the main program's is 1, a task's gets an id on its first stop.
(var- driver/main nil)
(def- driver/threads @{}) # task fiber -> id
(var- driver/last-thread 1)
# ponytail: one stop is served at a time, the others wait on this lock to report theirs.
(def- driver/serving (ev/lock))

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
    (eachp [line breakpoint] (or (driver/wanted path) {})
      (when (seen line) (driver/break path line (breakpoint :id))))))

(defn- driver/set-breakpoints
  "Replaces the breakpoints of `path` with `breakpoints` of [id line condition log-message], the
  last two nil when not given."
  [path breakpoints]
  (def path (driver/real path))
  (eachp [line [source column]] (or (driver/applied path) {})
    (protect (debug/unbreak source line column)))
  (put driver/applied path @{})
  (def wanted @{})
  (each [id line condition log] breakpoints
    (put wanted line {:id id :condition condition :log log}))
  (put driver/wanted path wanted)
  (eachp [line breakpoint] wanted
    (when (get-in driver/lines [path line])
      (driver/break path line (breakpoint :id)))))

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

(defn- driver/key-name [key]
  (if (symbol? key) (string key) (string/format "%q" key)))

(defn- driver/children [value]
  (if (dictionary? value)
    (map |(driver/variable (driver/key-name $) (in value $))
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

(defn- driver/value
  "The value of `code` with the locals of `frame`, if any, over its module's env. Writes to locals
  stay in the evaluation."
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
    :dead result
    :debug (error "the evaluation hit a breakpoint")
    (error (string result))))

(defn- driver/evaluate [frame code]
  (def result (driver/value frame code))
  {:result (driver/show result 20) :variablesReference (driver/ref result)})

(defn- driver/set-variable
  "Puts the value of `code`, evaluated in the top frame, under `name` in the table or array behind
  `ref`. Locals are copies of the frame's slots, which Janet cannot write back."
  [ref name code]
  (def container (get driver/refs (- ref 1)))
  (when (find |(= container (get $ :locals)) driver/stack)
    (error "Janet cannot change a local of a stopped frame, only the tables and arrays it holds"))
  (unless (index-of (type container) [:table :array])
    (error (string "a " (type container) " cannot change")))
  (def key (if (table? container)
             (find |(= name (driver/key-name $)) (keys container))
             (scan-number name)))
  (when (nil? key) (error (string "no " name " here")))
  (def value (driver/value (get driver/stack 0) code))
  (put container key value)
  (driver/variable name value))

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
    :setVariable (driver/set-variable ;args)
    (error (string "unknown command " name))))

(defn- driver/interpolate
  "Logpoint `message` with each `{expression}` replaced by its value in `frame`."
  [frame message]
  (defn value [_ code]
    (try
      (let [value (driver/value frame code)]
        (if (bytes? value) (string value) (driver/show value 4)))
      ([err] (string "<" err ">"))))
  (peg/replace-all ~(* "{" (<- (to "}")) "}") value message))

(defn- driver/passes?
  "Whether `f`, stopped at a breakpoint, goes on: a logpoint logs its message, and a condition that
  is false or nil lets it through. A condition that fails to evaluate stops."
  [f]
  (def frame (first (driver/frames f)))
  (def source (get frame :source))
  (def breakpoint (when (string? source)
                    (get-in driver/wanted [(driver/real source) (frame :source-line)])))
  (cond
    (nil? breakpoint) false
    (breakpoint :log) (do
                        (driver/send {:event "output"
                                      :output (string (driver/interpolate frame (breakpoint :log)) "\n")})
                        true)
    (breakpoint :condition) (not (try (driver/value frame (breakpoint :condition)) ([_] true)))
    false))

### Running and stepping

(defn- driver/catch-pauses []
  # ponytail: no signals on Windows, so no pause there.
  (unless (= :windows (os/which))
    (os/sigaction :usr1 (fn [] (set driver/pausing true)) true)))

(defn- driver/halted?
  "Whether `f` stopped at a breakpoint or for a pause."
  [f]
  (index-of (fiber/status f) [:debug :interrupted]))

(defn- driver/resume
  "Resumes `f` until it ends, hits a breakpoint or is paused. An interrupt the adapter did not ask
  for, one that came while nothing ran, is passed over."
  [f]
  (var value (resume f))
  (while (and (= :interrupted (fiber/status f)) (not driver/pausing))
    (set value (resume f)))
  (set driver/pausing false)
  value)

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
               (driver/resume f)))
  (def top (first (driver/frames f)))
  [value (or (not (driver/halted? f))
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
    [value (or (not (driver/halted? f)) (<= (length (driver/frames f)) depth))])
  (defn finished []
    (def value (driver/resume f))
    [value (not (driver/halted? f))])
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
    (def [d l] (if (driver/halted? f) (driver/position f) [0 nil]))
    (cond
      (= :interrupted (fiber/status f)) (set result [value "pause"])
      (not (driver/halted? f))
      (do
        # A launched program goes on to its next top-level form; stop there.
        (when (and (= driver/mode :launch) (= :dead (fiber/status f)) (= f driver/main))
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
  (if (driver/halted? f) [(driver/resume f) "breakpoint"] [nil nil]))

(defn- driver/thread
  "The thread id of `f`, given on its first stop."
  [f]
  (cond
    (= f driver/main) 1
    (driver/threads f) (driver/threads f)
    (do (put driver/threads f (++ driver/last-thread)) driver/last-thread)))

(defn- driver/stopped
  "Reports the stop of `f` and answers commands about it until one resumes it. Returns the name of
  that command, or :disconnected once the adapter is gone."
  [f reason text]
  (def fresh @{})
  (driver/scan-modules fresh)
  (driver/apply fresh)
  (set driver/stack (driver/frames f))
  (file/flush stdout)
  (file/flush stderr)
  (def thread (driver/thread f))
  (driver/send (merge {:event "stopped" :reason reason :threadId thread
                       :name (if (one? thread) "main" (string "task " thread))}
                      (if text {:text text} {})))
  (var result nil)
  (while (nil? result)
    (def command (ev/take driver/commands))
    (if (nil? command)
      (set result :disconnected)
      (let [[id name & args] command]
        (if (index-of name [:continue :next :stepIn :stepOut])
          (do
            (driver/send {:id id :body {}})
            (array/clear driver/refs)
            (set driver/stack @[])
            (set result name))
          (try
            (driver/send {:id id :body (driver/answer name args)})
            ([err] (driver/send {:id id :error (string err)})))))))
  result)

(defn- driver/serve
  "Serves the stop of `f` for `reason`, then resumes `f` as the command that ends it asks, outside
  the lock: the resumed code may wait on a task that stops next. Returns [value reason] of that
  resume."
  [f reason text]
  (if (and (= reason "breakpoint") (= :debug (fiber/status f)) (driver/passes? f))
    [(driver/resume f) "breakpoint"]
    (do
      (ev/acquire-lock driver/serving)
      (def reason (if (= :interrupted (fiber/status f)) "pause" reason))
      (def command (defer (ev/release-lock driver/serving) (driver/stopped f reason text)))
      (cond
        (= command :disconnected) (driver/disconnected f)
        (not (driver/halted? f)) [nil nil]
        (= command :continue) [(driver/resume f) "breakpoint"]
        (driver/step f command)))))

(defn- driver/trace [f err]
  (def buf @"")
  (with-dyns [:err buf] (debug/stacktrace f err ""))
  (string buf))

(defn driver/run
  "Runs `thunk` in a debug fiber: breakpoints stop it, and so do errors while `uncaught` is on.
  `entry` is the function a pending stop at the start breaks in. Errors propagate after the stop."
  [thunk &opt entry]
  (def launch (= driver/mode :launch))
  (def f (fiber/new thunk (if (or driver/uncaught launch) :deir :dir)))
  (set driver/main f)
  (var outcome
    (if-let [reason driver/at-start]
      (do
        (set driver/at-start nil)
        (def [value arrived] (driver/resume-to f (or entry thunk) 0))
        [value (if arrived reason "breakpoint")])
      [(driver/resume f) "breakpoint"]))
  (while (driver/halted? f)
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

### Tasks
#
# Tasks the loop runs are root fibers: their :debug signal reaches the loop, which reports it as an
# error and drops the task. Program code compiled while the debugger is on spawns tasks through
# these replacements of the root env's bindings, which run each task's body under `driver/task`.

(defn- driver/task
  "Runs `thunk`, the body of an event loop task, in a debug fiber: a breakpoint stops the task
  instead of ending it, and so does an error while `uncaught` is on. Errors propagate after the
  stop, to the loop or the task's supervisor."
  [thunk]
  (def f (fiber/new thunk (if driver/uncaught :deir :dir)))
  (var outcome [(driver/resume f) "breakpoint"])
  (while (driver/halted? f)
    (set outcome (driver/serve f (outcome 1) nil)))
  (when (and (= :error (fiber/status f)) driver/uncaught)
    (driver/serve f "exception" (driver/trace f (outcome 0))))
  (when-let [thread (driver/threads f)]
    (put driver/threads f nil)
    (driver/send {:event "thread" :reason "exited" :threadId thread}))
  (if (= :error (fiber/status f)) (propagate (outcome 0) f) (outcome 0)))

(defn- driver/go
  "`ev/go`, with a task function's body under `driver/task`."
  [task &opt value supervisor]
  (ev/go (if (= :function (type task))
           # `ev/go` passes `value` to a function of one argument.
           (fn :task [&] (driver/task (if (one? (disasm task :min-arity)) |(task value) task)))
           task)
         value
         supervisor))

(def- driver/tasks
  {'ev/go @{:value driver/go}
   'ev/spawn @{:macro true :value (fn :ev/spawn [& body] ~(,driver/go (fn :spawn [&] ,;body)))}
   'ev/call @{:value (fn :ev/call [f & args] (driver/go (fn :call [&] (f ;args))))}
   'net/server @{:value (fn :net/server [host port &opt handler & more]
                          (net/server host port
                                      (if (= :function (type handler))
                                        (fn :handler [conn] (driver/task |(handler conn)))
                                        handler)
                                      ;more))}})

# The root env's own bindings the replacements hide.
(def- driver/replaced @{})

(defn- driver/wrap-tasks []
  (eachk name driver/tasks
    (put driver/replaced name (in root-env name))
    (put root-env name (driver/tasks name))))

(defn- driver/unwrap-tasks []
  (eachp [name binding] driver/replaced
    (put root-env name binding))
  (table/clear driver/replaced))

### Connection

(defn- driver/read-commands []
  (def buf @"")
  (while (try (net/read driver/conn 65536 buf) ([_] nil))
    (while (def newline (string/find "\n" buf))
      (def command (parse (buffer/slice buf 0 newline)))
      (def [id name & args] command)
      (buffer/blit buf buf 0 (+ newline 1))
      (buffer/popn buf (+ newline 1))
      (case name
        :breakpoints (driver/set-breakpoints ;args)
        :exceptions (set driver/uncaught (args 0))
        # An evaluation while the program runs, in its env or the REPL's.
        :eval (driver/send (try {:id id :body {:result (driver/show (driver/value nil (args 0)) 20)
                                               :variablesReference 0}}
                                ([err] {:id id :error (string err)})))
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
  (def _reader (ev/spawn (driver/read-commands)))
  (def [_ _ program args stop-on-entry] (ev/take driver/commands))
  (def path (driver/real program))
  (def env (make-env root-env))
  (def subargs @[program ;args])
  (put env :args subargs)
  (put env :current-file path)
  (put env :source path)
  (set driver/env env)
  (driver/wrap-tasks)
  (driver/catch-pauses)
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
  (driver/unwrap-tasks)
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
  (driver/wrap-tasks)
  (driver/catch-pauses)
  (put env :janet-zed/debugger
       {:compiled driver/compiled :forget driver/forget :run driver/run})
  (ev/spawn
    (driver/read-commands)
    (driver/detach env))
  # netrepl replies in JDN, which has no fibers: a fiber result closes the connection.
  nil)
