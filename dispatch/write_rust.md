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

## Constant discipline (RULE UFS)

`audits/ufs.sh` reads `*.rs`. It did not until this pack said so, and the cost
was concrete: a crate spelled a wire verb inline twice — `pong == "PONG" ||
pong.contains("PONG")` — while `write_any.sh` printed a green UFS row over the
file, because the façade had always declared `*.rs` and the leaf had never read
it. Bind the repeated literal to a `const` or `static` and use the name.

Four Rust shapes are held out of the count, because no rename can fix them:

- **`#[cfg(test)]` and `#[test]` blocks.** Rust keeps unit tests inside the file
  they cover, so fixture keys would otherwise read as that file's worst debt.
- **Feature-gated test seams** — `#[cfg(feature = "test-util")]`. A helper that
  hands a sibling integration test one of each error kind has to compile into
  the crate, which is the only reason it is not behind `cfg(test)`; its contents
  are fixture data all the same. The match is narrow on purpose: `test` as a
  whole word or a `-`/`_` segment, so `#[cfg(feature = "redis")]` still counts.
- **Attribute literals** — `#[serde(rename = "…")]`, `#[cfg(feature = "…")]`,
  `#[doc = "…"]`. An attribute takes a literal token by language rule; a const
  is not accepted in that position.
- **`tests/` and `benches/`** at the crate root, which are fixture trees.

Everything else counts, including `examples/` — not because it compiles, but
because what repeats there is coupling, not demonstration. A cache namespace
declared at the top of a walkthrough and matched ninety lines below it, or a
fixture id that is both the fake store's match arm and the key five call sites
pass, breaks silently when one end moves. That is the drift RULE UFS exists to
catch, and example code is where a reader copies it from.

## Evolution

This pack is young next to the Zig façade and grows the same way: a Rust
incident worth keeping becomes a rule or **Example:** line via the
rule-extension protocol — never a memory file, never a milestone cite.
