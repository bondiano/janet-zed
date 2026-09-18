# One case of each type diagnostic, and the near misses that must stay quiet.
# The mistakes here are intentional. Do not fix them.

(comment :declare
  (def Circle :typedef {:kind :circle :r :number})
  (def Options :typedef {:retries :number & r})
  (def Registry :typedef @{:keyword :any})

  (defn host/fetch
    {:params [:string :number] :ret :string}
    "Read `n` bytes of the path the host resolves."
    [path n])

  (defn host/log
    {:params [:any :any] :ret :nil}
    "Whatever the host makes of it."
    [level message])

  (defn host/seek
    {:params [(or :string :number)] :ret :nil}
    "A place in the stream, named or counted."
    [where])

  (defn host/emit
    {:params [:string :any] :ret :nil}
    "A line and whatever else the caller has."
    [line & rest])

  (defn host/tag
    {:params [:string :keyword] :ret :nil}
    "A name and the tags on it: the last declared type is every rest argument's."
    [name & tags])

  (def host/limits
    {:type :struct}
    "What the host will not go over."
    nil)

  (def host/registry
    {:type Registry}
    "Whatever the host has registered, by name."
    nil))

(defn area
  {:params [Circle] :ret :number}
  "A key `Circle` does not have."
  [circle]
  (* 3.14 (circle :radius) (circle :r)))

(defn fetch-head
  "More arguments than the declaration takes."
  []
  (host/fetch "head.txt" 16 :binary))

(defn fetch-some
  "A string where the declaration takes a number."
  []
  (host/fetch "some.txt" "16"))

(defn every-limit
  "A struct called as if it were a function."
  []
  (host/limits))

(defn tag-badly
  "A string in a rest position the declaration types as a keyword."
  []
  (host/tag "release" :stable "beta"))

(defn fetch-named
  "A name bound to a string, where the declaration takes a number."
  []
  (def size "16")
  (host/fetch "some.txt" size))

(defn fetch-formatted
  "What a core function is written to return, where the declaration takes a number."
  []
  (host/fetch "some.txt" (string/format "%d" 16)))

(defn caption
  {:params [:number] :ret :string}
  "A number where the declaration returns a string."
  [n]
  (+ n 1))

(comment :declare
  (def Rect :typedef {:kind :rect :w :number :h :number})
  (def Shape :typedef (or Circle Rect))
  (def Event :typedef (or {:kind :click :x :number} {:kind :key :code :string} &))

  (defn host/draw
    {:params [Shape] :ret :nil}
    "Put a shape on the screen."
    [shape])

  (defn host/dispatch
    {:params [Event] :ret :nil}
    "Hand an event to whoever listens: the kinds listed, or any other."
    [event]))

(defn perimeter
  {:params [Shape] :ret :number}
  "A `case` without a default that names one kind of shape out of two."
  [shape]
  (case (shape :kind)
    :circle (* 2 3.14 (shape :r))))

(defn draw-square
  "A struct no member of `Shape` is: none of them is of `:kind :square`."
  []
  (host/draw {:kind :square :side 2}))

# Nothing below is a mistake.

(defn rejects-a-bad-kind
  "A test asserts that the wrong argument is rejected, so passing one is the point."
  []
  # janet-zed: ignore types
  (host/fetch "some.txt" "16"))


(defn summarize
  "No metadata: whatever inference reads of `n` is its own business."
  [n]
  (host/log :info (+ n 1)))

(defn quiet
  {:params [Options] :ret :nil}
  "`:any` and a union take anything, and an open form has every key."
  [options]
  (host/log :info (options :retries))
  (host/log 1 (options :whatever-else))
  # `@{:keyword :any}` is a dictionary keyed by name, not a form whose one key is `:keyword`,
  # so any name reads out of it.
  (host/registry :anything/at-all)
  (host/seek "start")
  (host/seek 0)
  (host/fetch "ok.txt" 16)
  # A rest parameter takes as many as the caller has, each of the type declared for it.
  (host/emit "done" 1 :two "three")
  (host/tag "release" :stable :beta :rc)
  # A name the core binds is left alone there: a DSL gives `*`, `+` and `?` its own meaning, and
  # PEG's `(* "task-" :w+)` is a sequence of strings, not multiplication of numbers.
  (* "task-" :w+)
  # Inference reads `summarize` as taking a number. Nobody wrote that down, so nobody can say
  # this is wrong either.
  (summarize "text")
  # One argument reads a key out of whatever the head is, function or not.
  (host/limits :timeout))

(defn sixteen
  "No metadata: what it returns is inference's reading, not anyone's word."
  []
  "16")

(defn guesses
  "Nothing here is static enough to be wrong for certain."
  [flag n]
  # What a function nobody typed returns.
  (host/fetch "ok.txt" (sixteen))
  # A parameter nobody typed.
  (host/fetch "ok.txt" n)
  # A `var`, which holds whatever a `set` puts in it.
  (var size "16")
  (set size 16)
  (host/fetch "ok.txt" size)
  # A union, some of whose members fit.
  (host/fetch "ok.txt" (if flag 16 "16")))

(defn radius
  {:params [Shape] :ret :nil}
  "A key only some members hold reads as `nil` out of the others."
  [shape]
  # `:number?`: a union, whose number may be what is there.
  (host/fetch "ok.txt" (shape :r))
  (match shape
    {:kind :circle :r r} (host/fetch "ok.txt" r)
    {:kind :rect} (host/log :info "no radius")))

(defn on-event
  {:params [Event] :ret :nil}
  "An open union: a `case` need not name every kind, and a kind nobody listed is no mistake."
  [event]
  (case (event :kind)
    :click (host/log :click (event :x)))
  (when (= (event :kind) :scroll)
    (host/log :scroll event))
  (host/dispatch {:kind :scroll :dy 1}))
