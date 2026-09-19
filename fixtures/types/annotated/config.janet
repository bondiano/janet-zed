# Mutable tables filled in over time and handed to functions that declare what they take;
# optional, named and keyed parameters; nested lookups that may come up empty.

(def Db :typedef {:host :string :pool :number})
(def Config :typedef '@{:host :string :port :number? :tags @[:keyword] :db Db?})
(def Counts :typedef @{:keyword :number})

(defn serve
  {:params [Config] :ret :string}
  "The address a config listens on."
  [cfg]
  (string (cfg :host) ":" (or (cfg :port) 80)))

(defn db-host
  {:params [Config] :ret :string?}
  "The database host, when a database is configured."
  [cfg]
  (-?> cfg (get :db) (get :host)))

(defn with-defaults
  {:params [@{:keyword :any}] :ret @{:keyword :any}}
  "The table with every key it does not set filled in."
  [t]
  (eachp [k v] {:host "localhost" :tags @[]}
    (when (nil? (t k)) (put t k v)))
  t)

(defn connect
  {:params [:string :number? :number?] :ret :string}
  "A connection string; the port and timeout are optional."
  [host &opt port timeout]
  (default port 5432)
  (default timeout 30)
  (string/format "%s:%d?timeout=%d" host port timeout))

(defn request
  {:params [:string :keyword :number? (or {:string :string} :nil)] :ret {:url :string :method :keyword :retries :number :headers {:string :string}}}
  "A request description, with named options."
  [url &named method retries headers]
  {:url url
   :method (or method :get)
   :retries (or retries 0)
   :headers (or headers {})})

(defn tagged
  {:params [:string :keyword] :ret :string}
  "A name followed by its tags."
  [name & tags]
  (string/join [name ;(map string tags)] " "))

(defn options
  {:params [(or :keyword :number :string)] :ret :table}
  "Keyed arguments collected into a table."
  [&keys opts]
  (table ;(kvs opts)))

(defn count-words
  {:params [[:keyword]] :ret Counts}
  "How often each keyword appears."
  [words]
  (def counts @{})
  (each w words
    (put counts w (inc (get counts w 0))))
  counts)

# The table starts with a port of nil and a host; both are set before it is served.
(def cfg @{:host "example.org" :port nil :tags @[]})
(put cfg :port 8080)
(array/push (cfg :tags) :web)
(set (cfg :host) "localhost")
(assert (= "localhost:8080" (serve cfg)))

(def later @{})
(put later :host "db.local")
(assert (= "db.local:80" (serve later)))
(put later :db {:host "pg" :pool 4})
(assert (= "pg" (db-host later)))
(assert (nil? (db-host cfg)))

(def filled (with-defaults @{:port 1}))
(assert (= "localhost" (filled :host)))

(assert (= "h:5432?timeout=30" (connect "h")))
(assert (= "h:1?timeout=2" (connect "h" 1 2)))
(assert (= :post ((request "/x" :method :post :retries 2) :method)))
(assert (= 0 ((request "/y") :retries)))
(assert (= "n a b" (tagged "n" :a :b)))
(assert (= 1 ((options :a 1 :b "two") :a)))

(def counts (count-words [:a :b :a]))
(assert (= 2 (counts :a)))
(update counts :b inc)
(assert (= 2 (counts :b)))

(var hits {:type :number} 0)
(for i 0 3 (+= hits i))
(assert (= 3 hits))

(def state {:type @{:seen @[:string] :last :string?}} @{:seen @[] :last nil})
(each word ["a" "b"]
  (array/push (state :seen) word)
  (put state :last word))
(assert (= "b" (state :last)))

# Literals written empty-handed and filled in: by `set` of a key, `put` into an array, and a
# top-level `var` set before it is used.
(def limits @{:max nil})
(set (limits :max) 10)

(defn limit
  {:params [@{:max :number}] :ret :number}
  "The limit a table sets."
  [t]
  (t :max))

(assert (= 10 (limit limits)))

(def names @[nil nil])
(put names 0 "a")
(put names 1 "b")

(defn joined
  {:params [@[:string]] :ret :string}
  "The strings, comma separated."
  [xs]
  (string/join xs ","))

(assert (= "a,b" (joined names)))

(defn loud
  {:params [:string] :ret :string}
  "The text in capitals."
  [s]
  (string/ascii-upper s))

(var greeting nil)
(set greeting "hi")
(assert (= "HI" (loud greeting)))
