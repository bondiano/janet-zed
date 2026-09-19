; Zed indents a new line by whole levels (tab_size) from the line an open bracket is on, and keeps
; the previous line's indentation otherwise; it has no alignment captures. spork/fmt indents the
; body of `defn`, `let` and the other control forms two past their `(`, which this matches when the
; form starts its line. It aligns other calls to their first argument and `[…]` / `{…}` one past
; the bracket, which no query can express: see "Indentation" in the README.
(_ ")" @end) @indent
(_ "]" @end) @indent
(_ "}" @end) @indent
