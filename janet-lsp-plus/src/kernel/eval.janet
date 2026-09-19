# Called through netrepl's 0xFF channel as `(this-fn code source line column client)`: evaluates
# `code` in the shared REPL env (the proto of the connection's fiber env) and returns
# [value output errors]. `source` is the file the kernel found `code` in, with its 1-based `line`
# and `column` there, or :zed. With the debugger attached (`dap/driver.janet`), thunks run through
# its hook, so breakpoints stop them. The server's SIGINT handler cancels the task named
# :janet-zed/evaluating.
#
# `client` names the connection in the server's :janet-zed/streams. Over it, before the reply,
# output goes as it is printed, as netrepl frames `\xFF<text>`, and `getline` asks for a line
# with `\xFE<prompt>`, answered by a frame holding the line (empty at the end of input). Output
# left when the evaluation ends comes in the reply.
(fn [code source line column client]
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
  (def stream (get-in env [:janet-zed/streams client]))
  # One frame at a time on the stream: the flusher's, or the prompt's.
  (def lock (ev/lock))
  (defn frame [bytes]
    (def message @"")
    (buffer/push-word message (length bytes))
    (:write stream (buffer/push message bytes)))
  # Until the reply: output flushed after it would break the protocol.
  (var streaming (not (nil? stream)))
  (defn flush []
    (ev/with-lock lock
      (when (and streaming (not (empty? output)))
        (def text (string "\xFF" output))
        (buffer/clear output)
        (frame text))))
  (defn ask [question]
    (flush)
    (ev/with-lock lock
      (frame (string "\xFE" question))
      (def head @"")
      (unless (:chunk stream 4 head) (error "the kernel went away"))
      (def [b0 b1 b2 b3] head)
      (def size (+ b0 (* b1 0x100) (* b2 0x10000) (* b3 0x1000000)))
      (def answer @"")
      (when (pos? size) (:chunk stream size answer))
      answer))
  # ponytail: output goes out while the evaluation waits on the event loop; a busy loop that never
  # yields sends it all at the end.
  # Cancelling it would end the connection: netrepl's nursery supervises the tasks it spawns.
  (when streaming (ev/spawn (while streaming (ev/sleep 0.05) (flush))))
  (put env :janet-zed/evaluating (fiber/root))
  (when stream (put env :janet-zed/input ask))
  (defer (do
           (put env :janet-zed/evaluating nil)
           (put env :janet-zed/input nil)
           (ev/with-lock lock (set streaming false)))
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
       :on-parse-error (into errors bad-parse)}))
  [value (string output) (string errors)])
