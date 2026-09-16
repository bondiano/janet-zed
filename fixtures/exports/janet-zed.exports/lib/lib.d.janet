# Declarations the library ships beside its `config.jdn`.

(def Thing :typedef {:id :number :label :string})

(defn lib/render
  {:params [Thing] :ret :string}
  "How a thing reads."
  [thing])
