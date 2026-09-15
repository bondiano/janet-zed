# One JSON line per binding: name, kind, docstring, source map, and `project` for the bindings
# `project.janet` gets on top of root-env (`project/envs`). Scripts run in root-env itself, so
# the private helpers they define (json/encode) are skipped; core has no private bindings there.
# janet-zed: include ./json.janet ./project.janet

(defn- dump/rows [env project]
  (eachp [name binding] env
    # A project environment evaluates code rather than exporting it: janet-pm's `merge-module`
    # marks every binding it copies private.
    (when (and (symbol? name) (table? binding)
               (if project (nil? (in root-env name)) (not (binding :private))))
      (def value (binding :value))
      (print (json/encode
               @{:name name
                 :kind (cond
                         (binding :macro) "macro"
                         (binding :ref) "var"
                         (cfunction? value) "cfunction"
                         (function? value) "function"
                         "value")
                 :doc (binding :doc)
                 :sm (binding :source-map)
                 :project (if project true)})))))

# Root-env first, before loading the tools defines anything.
(dump/rows root-env false)
(each env (project/envs)
  (dump/rows env true))
