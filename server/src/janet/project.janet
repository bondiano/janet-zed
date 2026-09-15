# The environments jpm and janet-pm (`spork/declare-cc`) evaluate `project.janet` in, for the
# tools that are installed.

(defn- project/jpm []
  ((module/value (require "jpm/config") 'load-config)
    (module/value (require "jpm/default-config") 'config) false)
  ((module/value (require "jpm/pm") 'make-jpm-env)))

(defn- project/janet-pm []
  ((module/value (require "spork/declare-cc") 'jpm-shim-env) (make-env)))

# jpm's first: a name both define ends up with janet-pm's binding.
(defn- project/envs []
  (seq [make :in [project/jpm project/janet-pm]
        :let [[ok env] (protect (make))]
        :when ok]
    env))
