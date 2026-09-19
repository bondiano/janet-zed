# Polymorphic signatures, bounded variables, higher-order functions, predicates that narrow,
# typed values that change over time, and dictionaries.

(def Method :typedef '(enum :get :post :put))
(def Tally :typedef @{:string :number})

(defn map-into
  {:params [[a] (fn [a] b)] :ret @[b]}
  "Each element through `f`, into a new array."
  [xs f]
  (def out @[])
  (each x xs (array/push out (f x)))
  out)

(defn find-first
  {:params [(fn [a] :any) [a]] :ret a?}
  "The first element `pred` holds for."
  [pred xs]
  (var found nil)
  (each x xs
    (when (and (nil? found) (pred x))
      (set found x)))
  found)

(defn clamp
  {:params [a a a] :ret a :where {a :number}}
  "`x` kept between `lo` and `hi`."
  [x lo hi]
  (max lo (min hi x)))

(defn text-length
  {:params [(or :string :buffer :keyword :symbol)] :ret :number}
  "How long a name or a text is."
  [x]
  (cond
    (string? x) (length x)
    (buffer? x) (length x)
    (keyword? x) (length (string x))
    (length (string x))))

(defn positive?
  {:params [:any] :ret :boolean :narrows :number}
  "Whether `x` is a number above zero."
  [x]
  (and (number? x) (pos? x)))

(defn verb
  {:params [Method] :ret :string}
  "A method as the request line writes it."
  [m]
  (string/ascii-upper (string m)))

(defn tally
  {:params [[:string]] :ret Tally}
  "How often each word appears."
  [words]
  (def out @{})
  (each w words (put out w (+ 1 (get out w 0))))
  out)

(defn apply-twice
  {:params [(fn [:number] :number) :number] :ret :number}
  "`f` applied twice."
  [f x]
  (f (f x)))

(defn env-or
  {:params [:string :string] :ret :string}
  "The environment variable, or a fallback."
  [name fallback]
  (or (os/getenv name) fallback))

(assert (deep= @[2 4 6] (map-into [1 2 3] |(* 2 $))))
(assert (deep= @["1" "2"] (map-into [1 2] string)))
(assert (= 3 (find-first odd? [2 3 5])))
(assert (nil? (find-first |(> $ 9) [1 2])))
(assert (= "b" (find-first |(= $ "b") ["a" "b"])))
(assert (= 5 (clamp 9 0 5)))
(assert (= 3 (text-length :abc)))
(assert (= 2 (text-length @"ab")))
(assert (= 1 (text-length 'x)))

(def samples [1 -2 "three" 4 nil])
(var total {:type :number} 0)
(each v samples
  (when (positive? v)
    (+= total v)))
(assert (= 5 total))

(def method (if (os/getenv "ANNOTATED_POST") :post :get))
(assert (= "GET" (verb method)))
(assert (= "PUT" (verb :put)))
(def methods {:type @{:string Method}} @{})
(put methods "fetch" :get)
(assert (= "GET" (verb (methods "fetch"))))

(def words (tally ["a" "b" "a"]))
(assert (= 2 (words "a")))
(update words "c" |(inc (or $ 0)))
(assert (= 1 (words "c")))
(loop [[w n] :pairs words]
  (assert (string? w))
  (assert (number? n)))

(assert (= 9 (apply-twice |(+ $ 3) 3)))
(assert (= 3 (apply-twice inc 1)))
(assert (= 2 (apply-twice math/floor 2.5)))
(assert (= "fallback" (env-or "ANNOTATED_NO_SUCH_VARIABLE" "fallback")))

(var last-word {:type :string?} nil)
(each w ["x" "y"] (set last-word w))
(assert (= "Y" (string/ascii-upper (or last-word ""))))
(when last-word
  (assert (= 1 (text-length last-word))))

(def seen {:type @[:string]} @[])
(array/push seen "one")
(array/concat seen ["two" "three"])
(assert (= 3 (length seen)))
(def firsts {:type [:string]} (map |(string/slice $ 0 1) seen))
(assert (= "ott" (string/join firsts)))

(def matrix {:type @[@[:number]]} @[@[1 2] @[3 4]])
(put-in matrix [0 1] 20)
(update-in matrix [1 0] * 10)
(assert (deep= @[@[1 20] @[30 4]] matrix))

(def settings {:type @{:keyword :any}} @{:depth 1})
(merge-into settings {:depth 2 :name "x"})
(assert (= 2 (settings :depth)))
(def frozen {:type {:keyword :any}} (table/to-struct settings))
(assert (= "x" (frozen :name)))
