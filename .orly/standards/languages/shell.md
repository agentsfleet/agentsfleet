# Shell

This standard covers shell scripts, Git hooks and the commands an agent runs in zsh.
A coding agent reads it before writing a `*.sh` file or a multi-step command, and a reviewer judges a diff against it.

## Scripts

### SH-QUOTE — Every parameter expansion and command substitution is double-quoted.
- **Applies to:** `**/*.sh`
- **Checked by:** linter: shellcheck SC2086; shellcheck SC2046
- **Bad:** `cp $src $dest`, or `ls $(git ls-files)`
- **Good:** `cp "$src" "$dest"`
- **Why:** an unquoted path with a space or a glob character splits into several arguments or expands into file names.

### SH-ARRAY — An argument list is an array expanded as `"${args[@]}"`, never a string split on spaces.
- **Applies to:** `**/*.sh`
- **Checked by:** review
- **Bad:** `flags="--all --quiet"; git fetch $flags`
- **Good:** `flags=(--all --quiet); git fetch "${flags[@]}"`
- **Why:** a string cannot hold an argument that contains a space, and an array keeps each argument whole.

### SH-TRAP — A temporary file or directory is removed by an `EXIT` trap set right after it is created.
- **Applies to:** `**/*.sh`
- **Checked by:** review
- **Bad:** `tmp=$(mktemp); build > "$tmp"; rm "$tmp"`
- **Good:** `tmp=$(mktemp); trap 'rm -f "$tmp"' EXIT`
- **Why:** an error or an interrupt between creation and the last line skips a cleanup written at the end.

### SH-EVAL — External input never reaches `eval`.
- **Applies to:** `**/*.sh`
- **Checked by:** review
- **Bad:** `eval "$1"`
- **Good:** `case "$1" in build|test) make "$1" ;; *) echo "unknown: $1" >&2; exit 2 ;; esac`
- **Why:** `eval` runs whatever the input says, so the input becomes code.

### SH-COMPAT — A script uses only features of the shell its shebang names, at the oldest version the repository supports, which is Bash 3.2 where macOS runs it.
- **Applies to:** `**/*.sh`
- **Checked by:** review
- **Bad:** `mapfile -t files < <(git ls-files)` under `#!/bin/bash`
- **Good:** `while IFS= read -r file; do files+=("$file"); done < <(git ls-files)`
- **Why:** macOS ships Bash 3.2, which has no `mapfile`, no `declare -A` and no `${var,,}`.

### SH-EMPTY — Under `set -euo pipefail`, an empty array and a grep that matches nothing are handled explicitly, and every extraction asserts a minimum match count.
- **Applies to:** `**/*.sh`
- **Checked by:** review
- **Bad:** `"${files[@]}"` on an empty array, which Bash 3.2 rejects with `files[@]: unbound variable`
- **Good:** `${files[@]+"${files[@]}"}`, and `[ "${#ids[@]}" -ge 1 ] || exit 1` after filling `ids`
- **Why:** the script dies on an empty input, or an extraction that found nothing passes as success.

A `grep` that matches nothing exits 1, so one that may legitimately find nothing ends in `|| true`.

### SH-HOOKINDEX — A Git hook saves `GIT_INDEX_FILE` before clearing it, and reads the staged tree through the saved path.
- **Applies to:** `.githooks/*`, `.git/hooks/*`, and scripts a hook calls
- **Checked by:** review
- **Bad:** `unset GIT_INDEX_FILE`
- **Good:** `ORLY_INDEX_FILE="$GIT_INDEX_FILE"; unset GIT_INDEX_FILE`
- **Why:** `git commit -a` hands hooks a temporary index, so a hook that reads the default index sees nothing staged.

## Commands in zsh

### SH-COLON — A variable followed by a colon is braced, and a heredoc that carries code quotes its delimiter.
- **Applies to:** `**/*.sh`, `**/*.zsh`, and commands an agent runs in zsh
- **Checked by:** review
- **Bad:** `git show "$base:app/run.sh"`, or a script body under `<<EOF`
- **Good:** `git show "${base}:app/run.sh"`, and `<<'EOF'`
- **Why:** zsh reads `:a`, `:h` and `:t` after a bare name as modifiers, and an unquoted heredoc expands `$` inside the code.

With `base=main`, zsh expands `"$base:app/run.sh"` to the absolute path of `main` followed by `pp/run.sh`.

### SH-ZSHSPLIT — A command an agent runs in zsh never relies on word-splitting `$VAR` or on `PIPESTATUS`, and multi-step logic runs in `bash -c` or a bash script.
- **Applies to:** commands an agent runs in zsh
- **Checked by:** review
- **Bad:** `T="cargo test"; $T`, or `cargo test | tail; echo "${PIPESTATUS[0]}"`
- **Good:** `bash run.sh`, or `cargo test > test.log 2>&1; status=$?` before reading the log
- **Why:** zsh passes an unquoted `$T` as one word, so it looks for a command named `cargo test`, and its `PIPESTATUS` expands empty.
