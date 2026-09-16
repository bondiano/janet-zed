# The host API in use: `db/pull`, `db/q` and `db/transact` come from `host.d.janet`, with no
# `# janet-zed: declare` directive and no import.

(def schema
  "What a person is made of."
  {:person/name {:db/type :string}
   :person/age {:db/type :number}
   :person/email {:db/type :string :db/unique :identity}})

(defn get-person
  "The person `:id` names, or a 404 body."
  [request]
  (def eid (scan-number (get-in request [:params :id] "0")))
  (if-let [person (db/pull [:person/name :person/age :person/email] eid)]
    {:body person}
    {:status 404 :body {:error "no such person"}}))

(defn list-people
  "Every person at least `:min-age` old."
  [request]
  (def min-age (scan-number (get-in request [:query :min-age] "0")))
  (def rows (db/q ~[:find e name age :where [e :person/age age]] min-age))
  {:body (map (fn [[id name age]] {:id id :name name :age age}) rows)})

(defn add-person
  "Create a person and answer with its id."
  [request]
  (def {:email email :name name :age age} (request :body))
  (def person {:db/id "new" :person/name name :person/age age :person/email email})
  (def {:tempids ids} (db/transact [person]))
  {:status 201 :body {:id (ids "new")}})
