# Ambient declarations: the host API every file of this workspace sees. Never loaded, never
# imported, only read.

(def Eid :typedef :number)
(def Entity :typedef {:db/id Eid & r})
(def TxReport :typedef {:tx Eid :tempids {:string Eid}})

(defn db/pull
  {:params [[:keyword] Eid] :ret Entity? :throws [:db/not-found]}
  "Entity attributes selected by a pull pattern."
  [pattern eid])

(defn db/q
  {:params [:tuple :any] :ret [[:any]]}
  "Datalog query over the db."
  [query & inputs])

(defn db/transact
  {:params [[:any]] :ret TxReport :throws [:db/unique-violation]}
  "Apply tx-data and return what it changed."
  [tx-data])
