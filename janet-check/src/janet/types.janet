# A binding's type metadata written back as the type syntax of declaration files, for the host to
# read. Janet keeps a literal `[:number]` bracketed, but one it evaluated arrives with
# parentheses, and parentheses in the type syntax mean a call: only `or`, `fn` and `enum` are
# written that way. Keys of a form are sorted: their order in a struct is not the source's.
(defn- types/literal [x]
  (defn all [items] (string/join (map types/literal items) " "))
  (defn fields [open dict close]
    (def written (sort (seq [[key value] :pairs dict :when (not= key '&)]
                         (string (types/literal key) " " (types/literal value)))))
    (when-let [row (get dict '&)]
      (array/push written (string "& " (types/literal row))))
    (string open (string/join written " ") close))
  (case (type x)
    :tuple (if (and (= :parens (tuple/type x)) (index-of (first x) '[or fn enum]))
             (string "(" (all x) ")")
             (string "[" (all x) "]"))
    :array (string "@[" (all x) "]")
    :struct (fields "{" x "}")
    :table (fields "@{" x "}")
    (string/format "%q" x)))

# The keys of a definition's metadata that declare types.
(def- types/keys [:params :ret :throws :narrows :type])

# What `binding` declares, as one metadata struct, or nil when it declares nothing.
(defn- types/declared [binding]
  (def declared (mapcat |(if-let [value (get binding $)] [$ value] []) types/keys))
  (unless (empty? declared) (types/literal (struct ;declared))))
