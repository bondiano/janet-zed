# Types

A reference for the type language `janet-check` reads: every form a type can take, what it
means, and what the checker makes of it. [How the type checker works](type-checking.md) says how
it gets there.

Every example was run through the checker. A comment after a form shows what it answers:

```janet
(def n 42)                 # n : :number               — the type inference gives it
(method :put)              # ! method takes (enum :get :post) here, given :put
                           #                           — a finding, as janet-check prints it
```

- [Common types](#common-types)
  - [Any and Never](#any-and-never)
  - [Nil and nullable types](#nil-and-nullable-types)
  - [Atoms](#atoms)
  - [Keyword values](#keyword-values)
- [Type grammar](#type-grammar)
- [Types](#types)
  - [Unions](#unions)
  - [Enumerations](#enumerations)
  - [Tuples and arrays](#tuples-and-arrays)
  - [Structs and tables](#structs-and-tables)
  - [Open structs](#open-structs)
  - [Dictionaries](#dictionaries)
  - [Functions](#functions)
  - [Type variables](#type-variables)
  - [Named types](#named-types)
  - [Tagged unions](#tagged-unions)
  - [Open unions](#open-unions)
  - [Inferred types](#inferred-types)
- [Annotations](#annotations)
  - [Functions: `:params`, `:ret`, `:throws`](#functions-params-ret-throws)
  - [Predicates: `:narrows`](#predicates-narrows)
  - [Values: `:type`](#values-type)
  - [Casts: `:as-type`](#casts-as-type)
- [What fits where](#what-fits-where)

## Common types

A type is written as the value it stands for. Nothing has to be written: an unannotated file is
typed from its literals and calls, and every type shown in hover comes from somewhere.

### Any and Never

`:any` is every value. It fits everywhere and everything fits it, so a position that holds
`:any` is never complained about. It is what a type is when nobody knows, and what the core says
for a position nobody has typed.

`:never` is no value: what a form is that never returns. A function that only raises answers
`:never`, and a branch that only raises drops out of the `if` around it.

```janet
(defn fail [] (error "no"))              # fail : (fn [] :never)  throws :string
(defn safe [] (if (os/getenv "X") (fail) 1))
                                         # safe : (fn [] :number)  throws :string
```

### Nil and nullable types

`:nil` is the type of `nil` alone. `T?` is `T` or `nil`, written as a suffix on any atom or named
type, and the same type as `(or T :nil)`: the checker prints whichever is shorter.

```janet
(def nothing nil)                        # nothing : :nil
(defn pick [flag] (if flag "yes" nil))   # pick : (fn [:any] :string?)
```

An absent key and a key holding `nil` read the same in Janet, so `{:age :number?}` covers both.
There is no separate optional key.

`?` goes on an atom or a name, never on a form: `(or @[:string] :nil)` is written as a union.

### Atoms

An atom is what `(type x)` answers:

| Type | Values |
| --- | --- |
| `:nil` | `nil` |
| `:boolean` | `true`, `false` |
| `:number` | `42`, `3.14`, `math/inf` |
| `:string` | `"hi"` |
| `:buffer` | `@""` |
| `:keyword` | any keyword |
| `:symbol` | `'x` |
| `:function`, `:cfunction` | a function written in Janet, one written in C; each fits where the other is wanted |
| `:fiber` | `(fiber/new f)` |
| `:tuple`, `:array`, `:struct`, `:table` | any tuple, array, struct or table, whatever it holds |
| `:abstract` | a C value: a file, a compiled PEG, an integer box |
| `:pointer` | a raw C pointer |

```janet
(def n 42)        # n : :number
(def s "hi")      # s : :string
(def b true)      # b : :boolean
(def buf @"")     # buf : :buffer
(def sym 'x)      # sym : :symbol
```

`false` is not a type of its own: it is a `:boolean`.

### Keyword values

A keyword that is not an atom is the value itself: `:circle` is the type of `:circle` and nothing
else. A literal keeps its value, which is what [tagged unions](#tagged-unions) and
[enumerations](#enumerations) are made of.

```janet
(def k :circle)                                  # k : :circle
(defn three [x] (cond (= x 1) :one (= x 2) :two "many"))
                                                 # three : (fn [:any] (or :one :two :string))
```

In a type, a keyword named like an atom, `:number`, is always the atom.

## Type grammar

```
Type  = Atom | Atom "?"                     :number  :string?
      | Keyword                             :circle            a keyword value
      | Name | Name "?"                     Shape  Shape?      a :typedef; capitalised
      | var                                 a  b  r            a type variable; lowercase
      | "[" Type "]"                        [:number]          a tuple of any length
      | "[" Type Type+ "]"                  [:number :string]  a tuple of this shape
      | "@[" Type* "]"                      @[:string]         an array, as a tuple
      | "{" (Keyword Type)* Rest? "}"       {:r :number}       a struct
      | "@{" (Keyword Type)* Rest? "}"      @{:hits :number}   a table
      | "{" Atom Type "}"                   {:keyword :any}    a dictionary
      | "@{" Atom Type "}"                  @{:string :number} a dictionary, in a table
      | "(or" Type Type+ ")"                (or :string :number)
      | "(or" Type+ "&)"                    (or Click Key &)   an open union
      | "(enum" Keyword+ ")"                (enum :get :post)
      | "(fn [" Type* ("&" Type)? "]" Type ")"
                                            (fn [:number & :string] :nil)
      | "'" Type | "~" Type                 '{:a :number & r}  quoted; the same type
Rest  = "&" var
Atom  = :nil :boolean :number :string :buffer :keyword :symbol :function :cfunction :fiber
        :tuple :array :struct :table :abstract :pointer :any :never
```

A form that does not parse as a type declares nothing: it is neither shown nor held to anything.

## Types

### Unions

`(or A B …)` is a value of any of its members. Branches that answer different types join into
one:

```janet
(defn either [flag] (if flag 1 "one"))   # either : (fn [:any] (or :number :string))
```

Unions are kept in one normal form: flat, without repeats, `:never` dropped, and `:any`
swallowing the rest. `(or :string :nil)` is `:string?`, `(or :any :string)` is `:any`, and a
keyword an `enum` beside it already lists is not a member of its own.

A union given where one type is wanted is not complained about by default: some member fits,
and the checker cannot tell which one the program holds. `--strict` holds every member to it:

```janet
(defn takes-num {:params [:number]} [n] n)
(takes-num (if (os/getenv "X") 16 "16"))
# quiet by default
# --strict: ! takes-num takes :number here, given (or :number :string)
```

### Enumerations

`(enum :a :b …)` is one of these keywords. A literal outside it is a finding:

```janet
(defn method {:params [(enum :get :post)] :ret :string} [m] (string m))
(method :get)
(method :put)       # ! method takes (enum :get :post) here, given :put
```

Inside `method`, `m` is `(enum :get :post)`. Members are written as keywords: `(enum :get)`,
never `(enum get)`.

### Tuples and arrays

A tuple of one element type is a tuple of any length whose every element is that type. A tuple
of two or more is that shape exactly. An array is the same, mutable.

```janet
(def point [1 2])           # point : [:number :number]
(def mixed [1 "a"])         # mixed : [:number :string]
(def xs @[1 2 3])           # xs : @[:number :number :number]

(defn total {:params [[:number]] :ret :number} [xs] (sum xs))
(total [1 2 3])
(total [1 "2"])             # ! total takes [:number] here, given [:number :string]
```

A tuple and an array are compared by what they hold, not by kind: inference cannot always tell
which of the two a form it only indexed is. `[:number]` accepts `@[1 2]`.

### Structs and tables

`{:k T …}` is a struct with these keys; `@{:k T …}` a table. A literal is typed key by key:

```janet
(def circle {:kind :circle :r 1})     # circle : {:kind :circle :r :number}
(def counter @{:hits 0})              # counter : @{:hits :number}
```

A struct written without `& r` is closed: it has these keys and no others. Reading a key it does
not have from a named closed struct is a finding:

```janet
(def Circle :typedef {:kind :circle :r :number})
(defn area {:params [Circle] :ret :number} [c]
  (* 3.14 (c :radius)))               # ! :radius is not a key: this form has :kind :r
```

Where one shape is held against another, only the keys both have are compared: a struct with a
key more than wanted fits. A struct and a table are compared the same way, by what they hold.

### Open structs

`{:k T & r}` has these keys and some more nobody listed; `r` stands for the rest. It is what
inference reads a parameter as from the keys the body takes out of it:

```janet
(defn status [response] (response :status))
                                      # status : (fn [{:status a & r}] a)
(defn host [{:host h :port p}] [h p]) # host : (fn [{:host a :port b & r}] [a b])
```

In a file that runs, quote an open form: Janet compiles an unquoted `&` as a symbol and finds
none. `'…` and `~…` read the same, and the quote is spelling, not part of the type.

```janet
(def Options :typedef '{:retries :number & r})
(defn retry {:params [Options] :ret :number} [o] (o :retries))
(retry {:retries 3 :backoff 1})
(retry {:retries "3"})                # ! retry takes Options here, given {:retries :string}
```

### Dictionaries

A struct or table of one entry whose key is an atom is a dictionary: any key of the one type,
any value of the other. `{:keyword :number}` is keywords to numbers, not a struct with the key
`:keyword`.

```janet
(defn lookup {:params [{:keyword :number} :keyword] :ret :number?} [reg k] (get reg k))
(lookup {:a 1 :b 2} :a)
(lookup {:a "1"} :a)       # ! lookup takes {:keyword :number} here, given {:a :string}
```

A struct literal fits a dictionary when each of its values fits the value type. `@{:keyword :any}`
is the same dictionary in a table, the spelling a name-keyed registry is written with.

`tabseq` builds one from the key and value its body answers, and `:pairs` hands the body the key
and value types of what it walks:

```janet
(defn rebuild {:params [{:by :keyword :email :string}]} [creds]
  (tabseq [[k v] :pairs creds] k v))
# k : (enum :by :email)   v : (or :keyword :string)
# rebuild answers @{(enum :by :email) (or :keyword :string)}
```

### Functions

`(fn [A B] R)` takes an `A` and a `B` and answers an `R`; `(fn [A & T] R)` takes any number of
`T` after the `A`.

```janet
(defn twice {:params [(fn [:number] :number) :number] :ret :number} [f x] (f (f x)))
(twice inc 2)               # :number
```

Janet calls nearly anything: a struct, a keyword and a number read keys. A value is only
complained about as a callee when it can never take arguments — `nil` or a boolean — or when its
declared type is a form, called with other than one argument:

```janet
(comment :declare
  (def host/limits {:type :struct} "What the host will not go over." nil))

(host/limits)               # ! host/limits is :struct, not a function
```

What a function takes is not compared with what `(fn …)` wants: any function is quiet there.

### Type variables

A lowercase symbol is a type variable: it stands for one type, the same one each place it is
written in one signature, chosen afresh at each call.

```janet
(defn first-of {:params [[a]] :ret a?} [xs] (get xs 0))
(first-of [1 2])            # :number?
```

Inference generalises every definition on its own, so an unannotated function is polymorphic
and two calls do not run into each other:

```janet
(defn ident [x] x)          # ident : (fn [a] a)
(ident 1)                   # :number
(ident "a")                 # :string
```

A variable in a signature is never complained about: `a` holds whatever it is given. `r` in
`& r` is a variable too, the row of an open struct, and is what every row prints as.

### Named types

`(def Name :typedef T)` names a type. A name is capitalised, visible to its own file, and to every
file of the workspace when written in a [declaration file](../README.md#declaration-files). It
may refer to itself, and hover expands it.

```janet
(def Tree :typedef (or {:leaf :number} {:left Tree :right Tree}))
```

A `:typedef` is a real `def` in a file that runs: its value is the type literal, as data.

### Tagged unions

A union of structs that each hold a keyword of their own at one key is a tagged union. It has no
syntax of its own: the key and its keywords are enough.

```janet
(def Shape :typedef
  (or {:kind :circle :r :number}
      {:kind :rect :w :number :h :number}))
```

A test of the tag picks the member out, and a `case` or `match` with no default has to name
every tag:

```janet
(defn describe {:params [Shape]} [shape]
  (when (= (shape :kind) :circle)
    shape))                 # shape : {:kind :circle :r :number}

(defn area {:params [Shape] :ret :number} [shape]
  (case (shape :kind)       # ! case over Shape misses :rect
    :circle (* 3.14 (shape :r) (shape :r))))

(defn area2 {:params [Shape] :ret :number} [shape]
  (match shape
    {:kind :circle :r r} (* r r)          # r : :number
    {:kind :rect :w w :h h} (* w h)))     # quiet: both tags named
```

### Open unions

`(or A B &)` is one of these, or something nobody listed: for a set of kinds that grows. A tag
nobody listed narrows to an open struct, and no `case` is held to cover it:

```janet
(def Event :typedef (or {:kind :click :x :number} {:kind :key :code :number} &))

(defn handle {:params [Event]} [e]
  (case (e :kind)
    :click (e :x)))         # quiet: an open union has no last tag

(defn dragged {:params [Event]} [e]
  (when (= (e :kind) :drag)
    e))                     # e : {:kind :drag & r}
```

Nothing given where an open union is wanted is complained about, since it may be the member
nobody listed.

### Inferred types

A type inference reads out of a body rather than one someone wrote is a guess. It is shown in
hover like any other, marked `type inferred`, and it is never ground for a complaint, in either
mode — the checker speaks only for types someone wrote down.

```janet
(defn flag [x] (if x 16 "16"))            # flag : (fn [:any] (or :number :string))
(total [(flag 1)])                        # quiet, even under --strict: flag has no signature
```

The parameters and result of a function without a signature, a `var`, and what is read out of
a table or an array are guesses. So is anything read out of a guess. There is no way to write
one: an annotation is what makes a type more than a guess.

## Annotations

Types live in the metadata Janet already keeps for a definition, so they survive into the
compiled environment and a running REPL reports them back.

### Functions: `:params`, `:ret`, `:throws`

```janet
(defn parse
  {:params [:string] :ret :number :throws [:bad-input]}
  "Read a number, or raise."
  [s]
  (or (scan-number s) (error :bad-input)))
```

- `:params` is one type per parameter, in order. With `&` in the parameter vector, the last type
  belongs to the rest parameter and holds for every argument from its position on. A vector of
  the wrong length declares nothing.
- `:ret` is what a call answers. The body's last form is held to it.
- `:throws` is what the body raises. Without it, inference fills it in: `(error "empty")` in the
  body makes the function `throws :string`.

```janet
(defn log-all {:params [:string :keyword]} [msg & tags] msg)
(log-all "x" :a :b 3)       # ! log-all takes :keyword here, given :number
```

### Predicates: `:narrows`

`:narrows T` on a predicate says its first argument is a `T` wherever it answers truly, and is
not one wherever it does not. `:any` marks a predicate that tests a value rather than a type,
and so tells a branch nothing. The core's `string?`, `number?`, `nil?` and the rest are declared
this way; nothing is built in.

```janet
(defn positive? {:params [:any] :ret :boolean :narrows :number} [x]
  (and (number? x) (pos? x)))

(defn use-positive [x]
  (when (positive? x)
    x))                     # x : :number

(defn size {:params [:string?] :ret :number} [s]
  (if (string? s)
    (length s)              # s : :string
    0))                     # s : :nil
```

### Values: `:type`

`{:type T}` on a `def` or `var` makes `T` the name's type, and holds the value to it:

```janet
(def n {:type :string} 1)   # ! n is :number, declared :string; :as-type casts it
```

`T` stands over what the value infers to, which is what writing it is for: a value its
expression cannot type — a dictionary rebuilt key by key, what untyped code hands back — gets
the type written for it, and whatever reads the name reads `T`.

```janet
(def Credentials :typedef
  '{:by (or (enum :email :username :subject :id) :nil)
    :value :string? :password :string? & r})

(defn check {:params [(or {:any :any} :nil)]} [credentials0]
  (def credentials {:type Credentials}
    (tabseq [[k v] :pairs (or credentials0 {})]
      (if (bytes? k) (keyword k) k) v))    # quiet: the value is @{:any :any}
  (def by (or (credentials :by) :email))   # by : (enum :email :username :subject :id)
  by)
```

The value is held the way an argument is: `:any` and a guess fit, and a union is complained
about only under `--strict`.

```janet
(defn f [x] (def g {:type :string} x) g)  # quiet: x is :any
(def b {:type :string} (if (os/getenv "X") "x" 1))
# quiet by default
# --strict: ! b is (or :string :number), declared :string; :as-type casts it
```

In a [declaration file](../README.md#declaration-files) and a `(comment :declare …)` block,
`(def x {:type T} nil)` declares a value the host provides; the `nil` stands in for it and is
not held to anything.

### Casts: `:as-type`

`{:as-type T}` makes `T` the name's type whatever the value is, wider or narrower, and says
nothing. Janet is dynamic, and a program knows things no type does: that a union is one member
by now, that a table read from the outside has a shape.

```janet
(def c {:as-type :string} (if (os/getenv "X") "x" 1))   # c : :string, narrowed
(def d {:as-type :any} 1)                               # d : :any, widened
(def e {:as-type :number} "one")                        # e : :number, and no finding
```

`:type` is a claim the checker verifies; `:as-type` is one it takes on faith. Reach for
`:as-type` where `:type` complains about something the program guarantees, and keep it as close
to the value as possible: the cast is only as right as the reasoning beside it. Where both are
written, `:as-type` wins.

## What fits where

A finding needs certainty: both types written, or following from what is written, and no value
of one being a value of the other. Anything less is quiet.

| Given | Wanted | Answer |
| --- | --- | --- |
| anything | `:any` | fits |
| `:any`, a type variable, an inferred type | anything | quiet |
| `:never` | anything | fits |
| an atom | the same atom | fits; `:function` and `:cfunction` are alike |
| an atom | another atom | **finding** |
| `:circle` | `(enum :circle :rect)`, `:circle` | fits |
| `:square` | `(enum :circle :rect)`, `:circle` | **finding** |
| a union | anything | quiet; under `--strict`, a **finding** when some member does not fit |
| anything | a union | fits when some member fits, a **finding** when none can |
| anything | an open union | quiet unless it fits |
| `T` or `nil` | `T?` | fits |
| a tuple or array | a tuple or array | element by element; a **finding** when one cannot fit |
| a struct or table | a struct or table | key by key over the keys both have; a key more is fine |
| a struct | a dictionary | value by value |
| `nil`, a boolean | a function | **finding** |
| anything else | a function | quiet: Janet calls keywords, numbers and forms |
| a named type | anything | as what it names |

Against a core binding only a literal is measured: the core's declarations come out of Janet's
C sources, which say `:number` where they take an integer box too.

A line whose violation is the point — a test that passes a wrong argument to see it rejected —
says so with `# janet-zed: ignore types`; see
[comment directives](../README.md#comment-directives).
