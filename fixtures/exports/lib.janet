# The library itself, as `jpm_tree/lib/lib.janet` holds it. Its macros are the ones
# `janet-zed.exports/lib/config.jdn` lists under `:lint-as`: the source of a call names what it
# defines, and only the expansion the checker compiles carries the types.

(defmacro defthing
  "Define `name` as the thing `label` stands for."
  [name label]
  ~(def ,name {:type Thing} {:id 0 :label ,label}))

(defmacro shared
  "Define `name` as a function every module of the workspace shares."
  [name & body]
  ~(defn ,name {:params [:string] :ret :string} [text] ,;body))
