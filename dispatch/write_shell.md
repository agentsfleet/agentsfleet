# Shell authoring

Quote expansions, use arrays for argument lists, and clean temporary files on
every exit path. Avoid `eval` for external input. Match the repository's required
shell and macOS Bash compatibility level.

The repository owns ShellCheck and executable tests — it declares them in
`.oracle/orly.json`.
