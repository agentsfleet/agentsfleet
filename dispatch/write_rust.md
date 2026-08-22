# Rust authoring

Use ownership and borrowing to make resource lifetime visible. Keep `unsafe`
blocks small and state the invariant they rely on. Preserve error variants until
the caller has enough information to decide whether to retry or stop.

Test the feature combinations the repository actually builds. Concurrency work
needs a deterministic contention test, not only a happy-path asynchronous test.

The repository owns formatting, Clippy, build, test, security, and benchmark
commands — it declares them in `.oracle/orly.json`.

## Reference guideline (mandatory in review)

Microsoft's Pragmatic Rust Guidelines are this pack's depth — thorough,
agent-facing, and continuously maintained upstream. The local copy lives at
`~/Projects/oss/rust-guidelines/all.txt`; fetch it when absent:

```bash
mkdir -p ~/Projects/oss/rust-guidelines && \
  curl -fsSL https://microsoft.github.io/rust-guidelines/agents/all.txt \
  -o ~/Projects/oss/rust-guidelines/all.txt
```

Every Rust REVIEW — and any adversarial pass over `*.rs` — reads it
**sectioned, never whole-file** (186 sections):
`grep -nE "^#+ " ~/Projects/oss/rust-guidelines/all.txt`, read the sections
the diff touches, and cite the guideline IDs (`M-…` mnemonics, e.g.
`M-UNSOUND`, `M-SIMPLE-ABSTRACTIONS`) applied or consciously diverged from.
Local convention wins on conflict, but the divergence is named in the review
output. Refresh the copy when the user asks or a cited rule looks stale
(re-run the fetch; upstream updates continuously).

## Evolution

This pack is young next to the Zig façade and grows the same way: a Rust
incident worth keeping becomes a rule or **Example:** line via the
rule-extension protocol — never a memory file, never a milestone cite.
