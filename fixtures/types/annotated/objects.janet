# Objects the Janet way: tables with methods called as `(:m obj)`, prototypes set with
# `table/setproto`, and state kept in the table and changed by its methods.

(def Account :typedef '@{:owner :string :balance :number :history @[:number] & r})

(def account-proto
  @{:deposit (fn [self amount]
               (update self :balance + amount)
               (array/push (self :history) amount)
               self)
    :withdraw (fn [self amount]
                (if (> amount (self :balance))
                  (error "insufficient funds")
                  (do
                    (update self :balance - amount)
                    (array/push (self :history) (- amount))
                    self)))
    :describe (fn [self]
                (string/format "%s: %g" (self :owner) (self :balance)))})

(defn account
  {:params [:string :number?] :ret Account}
  "An account owned by `owner`, opened with a balance."
  [owner &opt opening]
  (table/setproto @{:owner owner :balance (or opening 0) :history @[]} account-proto))

(defn statement
  {:params [Account] :ret :string}
  "The history of an account, one movement a line."
  [acct]
  (string/join (map |(string/format "%+g" $) (acct :history)) "\n"))

(defn transfer
  {:params [Account Account :number] :ret :boolean}
  "Move money between accounts; false where the source cannot pay."
  [from to amount]
  (try
    (do
      (:withdraw from amount)
      (:deposit to amount)
      true)
    ([_] false)))

(def Counter :typedef '@{:n :number & r})

(def counter-proto
  @{:inc (fn [self] (++ (self :n)) self)
    :get (fn [self] (self :n))})

(defn counter
  {:params [:number?] :ret Counter}
  "A counter starting at `start`."
  [&opt start]
  (table/setproto @{:n (or start 0)} counter-proto))

(def a (account "ann" 10))
(def b (account "bo"))
(:deposit a 5)
(assert (= 15 (a :balance)))
(assert (transfer a b 7))
(assert (not (transfer b a 100)))
(assert (= "ann: 8" (:describe a)))
(assert (= "+5\n-7" (statement a)))
(assert (= account-proto (table/getproto b)))

(def c (counter))
(:inc c)
(:inc (:inc c))
(assert (= 3 (:get c)))
(assert (= 10 (:get (counter 10))))

# A prototype chain three deep: each level adds to what it inherits.
(def base @{:greet (fn [self] (string "hi " (self :name)))})
(def named (table/setproto @{:name "base"} base))
(def child (table/setproto @{:name "child"} named))
(assert (= "hi child" (:greet child)))
(assert (= "base" ((table/getproto child) :name)))
