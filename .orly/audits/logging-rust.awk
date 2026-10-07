# Rust tracing and diagnostic detection; the shell owns scope and findings.
    function count_char(text, char, copy, count) {
      copy = text
      count = gsub(char, "", copy)
      return count
    }
    function code_only(text, output, cursor, char, escaped, quoted) {
      output = ""
      escaped = 0
      quoted = 0
      for (cursor = 1; cursor <= length(text); cursor++) {
        char = substr(text, cursor, 1)
        if (quoted) {
          if (escaped) escaped = 0
          else if (char == "\\") escaped = 1
          else if (char == "\"") quoted = 0
          continue
        }
        if (char == "\"") {
          quoted = 1
          continue
        }
        if (char == "/" && substr(text, cursor + 1, 1) == "/") break
        output = output char
      }
      return output
    }
    function without_comment(text, output, cursor, char, escaped, quoted) {
      output = ""
      escaped = 0
      quoted = 0
      for (cursor = 1; cursor <= length(text); cursor++) {
        char = substr(text, cursor, 1)
        if (quoted) {
          output = output char
          if (escaped) escaped = 0
          else if (char == "\\") escaped = 1
          else if (char == "\"") quoted = 0
          continue
        }
        if (char == "\"") {
          quoted = 1
          output = output char
          continue
        }
        if (char == "/" && substr(text, cursor + 1, 1) == "/") break
        output = output char
      }
      return output
    }
    function comment_text(text, cursor, char, escaped, quoted) {
      escaped = 0
      quoted = 0
      for (cursor = 1; cursor <= length(text); cursor++) {
        char = substr(text, cursor, 1)
        if (quoted) {
          if (escaped) escaped = 0
          else if (char == "\\") escaped = 1
          else if (char == "\"") quoted = 0
          continue
        }
        if (char == "\"") {
          quoted = 1
          continue
        }
        if (char == "/" && substr(text, cursor + 1, 1) == "/")
          return substr(text, cursor + 2)
      }
      return ""
    }
    function has_logging_reason(text, comment) {
      comment = comment_text(text)
      return comment ~ /^[[:space:]]*logging:[[:space:]]*[^[:space:]]/
    }
    function check_emit() {
      if (emit_code !~ /event[[:space:]]*=/ && emit_code !~ /[(,][[:space:]]*event[[:space:]]*[,)]/)
        printf "event|%s|%d\n", FILENAME, emit_line
      if (emit_source ~ /"[^"\n]*\{[^"\n]*\}[^"\n]*"/)
        printf "positional|%s|%d\n", FILENAME, emit_line
      emit_code = ""
      emit_source = ""
      in_emit = 0
    }
    FNR == 1 {
      depth = 0
      test_depth = 0
      cfg_test = 0
      in_emit = 0
      emit_parens = 0
      emit_code = ""
      emit_source = ""
      previous_annotation = 0
    }
    {
      current_annotation = has_logging_reason($0)
      code = code_only($0)
      if (!in_emit && code ~ /^[[:space:]]*$/) {
        previous_annotation = current_annotation
        next
      }
      opens = count_char(code, "\\{")
      closes = count_char(code, "\\}")
      if (test_depth > 0) {
        depth += opens - closes
        if (depth < test_depth) test_depth = 0
        previous_annotation = current_annotation
        next
      }
      if (code ~ /^[[:space:]]*#\[cfg\(test\)\]/) {
        if (opens > 0) {
          test_depth = depth + 1
          depth += opens - closes
          if (depth < test_depth) test_depth = 0
        } else cfg_test = 1
        previous_annotation = current_annotation
        next
      }
      if (cfg_test) {
        if (opens > 0) {
          test_depth = depth + 1
          depth += opens - closes
          if (depth < test_depth) test_depth = 0
          cfg_test = 0
        } else if (code ~ /;/) cfg_test = 0
        previous_annotation = current_annotation
        next
      }
      if (code ~ /(^|[^[:alnum:]_])(println|eprintln|dbg)!/ && !current_annotation && !previous_annotation)
        printf "direct|%s|%d\n", FILENAME, FNR
      if (!in_emit && match(code, /tracing::(error|warn|info|debug|trace)!/)) {
        in_emit = 1
        emit_line = FNR
        emit_code = code
        emit_source = without_comment($0)
        macro = substr(code, RSTART)
        emit_parens = count_char(macro, "\\(") - count_char(macro, "\\)")
      } else if (in_emit) {
        emit_code = emit_code " " code
        emit_source = emit_source " " without_comment($0)
        emit_parens += count_char(code, "\\(") - count_char(code, "\\)")
      }
      if (in_emit && emit_parens <= 0) check_emit()
      depth += opens - closes
      previous_annotation = current_annotation
    }
