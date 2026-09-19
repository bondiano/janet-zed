# Results as tagged tuples, `[:ok v]` and `[:err e]`: built, matched, tested by their tag, and
# chained. Errors raised and caught; `and`, `or` and `-?>` as guards over values that may be nil.

(def Parsed :typedef '(or [:ok :number] [:err :string]))
(def Entry :typedef {:name :string :age :number? :email :string?})

(defn parse-age
  {:params [(or :string :buffer)] :ret Parsed}
  "The age a text spells, or why it spells none."
  [text]
  (def n (scan-number text))
  (cond
    (nil? n) [:err (string "not a number: " text)]
    (neg? n) [:err "negative"]
    [:ok n]))

(defn age-or
  {:params [Parsed :number] :ret :number}
  "The age a result holds, or the default."
  [result fallback]
  (match result
    [:ok n] n
    [:err _] fallback))

(defn failure
  {:params [Parsed] :ret :string?}
  "Why a result failed, or nil."
  [result]
  (when (= (result 0) :err)
    (result 1)))

(defn ok?
  {:params [Parsed] :ret :boolean}
  "Whether a result holds a value."
  [result]
  (= :ok (first result)))

(defn all-ages
  {:params [[:string]] :ret (or [:ok @[:number]] [:err :string])}
  "Every age, or the first failure."
  [texts]
  (def ages @[])
  (var failed nil)
  (each text texts
    (unless failed
      (match (parse-age text)
        [:ok n] (array/push ages n)
        [:err e] (set failed e))))
  (if failed [:err failed] [:ok ages]))

(defn checked-age
  {:params [:string] :ret :number :throws [:string]}
  "The age a text spells; raises when it spells none."
  [text]
  (match (parse-age text)
    [:ok n] n
    [:err e] (error e)))

(defn safe-age
  {:params [:string] :ret :number?}
  "The age, or nil where `checked-age` raises."
  [text]
  (try
    (checked-age text)
    ([err] nil)))

(defn domain
  {:params [Entry] :ret :string?}
  "The domain of an entry's email, when it has one."
  [entry]
  (when-let [email (entry :email)]
    (-?> (string/find "@" email)
         (inc)
         (|(string/slice email $))
         (string/ascii-lower))))

(defn greeting
  {:params [Entry] :ret :string}
  "A greeting that uses whatever the entry has."
  [entry]
  (def age (entry :age))
  (string "hello " (entry :name)
          (or (and age (> age 17) ", adult") "")
          (if-let [d (domain entry)] (string " at " d) "")))

(defn older
  {:params [Entry Entry] :ret Entry}
  "The older of two entries; one without an age loses."
  [a b]
  (def x (a :age))
  (def y (b :age))
  (if (and x (or (nil? y) (> x y))) a b))

(assert (deep= [:ok 42] (parse-age "42")))
(assert (deep= [:ok 7] (parse-age @"7")))
(assert (= :err (first (parse-age "x"))))
(assert (= 3 (age-or (parse-age "-1") 3)))
(assert (= "negative" (failure (parse-age "-1"))))
(assert (nil? (failure (parse-age "5"))))
(assert (ok? (parse-age "5")))
(assert (deep= [:ok @[1 2]] (all-ages ["1" "2"])))
(assert (= :err (first (all-ages ["1" "no"]))))
(assert (= 9 (checked-age "9")))
(assert (nil? (safe-age "nine")))
(assert (= "example.org" (domain {:name "a" :email "a@Example.org"})))
(assert (nil? (domain {:name "b"})))
(assert (= "hello ann, adult at x.io" (greeting {:name "ann" :age 30 :email "ann@x.io"})))
(assert (= "hello bo" (greeting {:name "bo" :age 3})))
(assert (= "ann" ((older {:name "ann" :age 30} {:name "bo"}) :name)))
