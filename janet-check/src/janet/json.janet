# A JSON encoder for the results scripts print; Janet's core has none.

(defn- json/string [s]
  (def out @"\"")
  (each byte s
    (case byte
      34 (buffer/push out "\\\"")
      92 (buffer/push out "\\\\")
      10 (buffer/push out "\\n")
      13 (buffer/push out "\\r")
      9 (buffer/push out "\\t")
      (if (< byte 32)
        (buffer/push out (string/format "\\u%04x" byte))
        (buffer/push-byte out byte))))
  (buffer/push out "\""))

(defn- json/encode [x]
  (case (type x)
    :nil "null"
    :boolean (if x "true" "false")
    :number (string/format "%j" x)
    :array (string "[" (string/join (map json/encode x) ",") "]")
    :tuple (string "[" (string/join (map json/encode x) ",") "]")
    :table (string "{" (string/join (seq [[k v] :pairs x]
                                      (string (json/string (string k)) ":" (json/encode v)))
                                    ",") "}")
    :struct (json/encode (table ;(kvs x)))
    (json/string (string x))))
