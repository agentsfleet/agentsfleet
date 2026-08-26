# Rust authoring

<!-- oracle-scope: *.rs -->

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

## Error discipline (RULE ERR-RS)

One error type per crate, and a `Result` alias beside it:

    pub type Result<T, E = Error> = core::result::Result<T, E>;

Every fallible function in the crate returns `Result<T>`. The default type
parameter is what lets the few functions answering with a foreign error keep
the same spelling — `Result<T, VerifyError>` — instead of reaching for
`std::result::Result`. A reader must never have to check WHICH error a
signature returns to know it is this crate's.

**Compose with `From`; `?` does the lifting.** This is `M-FROM-ERROR` and is
read there, carve-out included; what the guideline leaves open is how the
variant is spelled, which locally is `#[from]`:

    #[derive(Debug, thiserror::Error)]
    pub enum BootFailure {
        #[error(transparent)]
        Environment(#[from] Refusal),
        #[error("cannot boot: the database would not answer")]
        Database(#[from] db::Error),
    }

**`map_err` only to ADD context the call site alone knows** — a role tag, a
budget, an operation name; the same carve-out `M-FROM-ERROR` names. And never
this, which no guideline covers:

    // WRONG: a lossy conversion wearing a conversion's clothes.
    .map_err(|error| Mine::Database(error.to_string()))?

`to_string()` on the way INTO an error type destroys the `source()` chain. It
compiles, it reads fine, and it silently defeats every chain walker downstream.

**`source()` returns what caused you, never yourself.** An error whose
`Display` already renders its kind must not also return that kind as its
source, or a chain walker prints the same sentence twice before reaching
anything new:

    // WRONG — Display is "[code] {kind}", so the kind is already printed.
    fn source(&self) -> Option<&(dyn Error + 'static)> { Some(&self.kind) }

    // RIGHT — skip ourselves, hand back what the kind wraps.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        std::error::Error::source(&self.kind)
    }

**Not every error has a cause, and that is not a defect.** A variant holding
another error has one; a variant holding only data (`MissingUrl { knob }`) does
not — nothing *caused* an unset variable. A test asserting `source().is_some()`
for every variant is WRONG and forces authors to invent causes. Assert the real
invariant: where there IS a source, it is not a repeat of our own message.

**Divergence from the reference guideline, named as that section requires.**
`M-ERRORS-CANONICAL-STRUCTS` prescribes a situation-specific `struct` carrying
a `Backtrace` and lets a complex crate expose several error types;
`M-APP-ERROR` further lets an application crate skip its own type for
`anyhow`/`eyre`/`ohno::AppError`. This rule diverges on all three for one
reason: a `#[from]`-composed enum behind a single per-crate alias is what lets
`?` lift across a crate boundary with no call-site conversion, and an
application-level catch-all erases the variant the caller would have matched
on. A `Backtrace` inside a variant is welcome. A crate with two genuinely
unrelated fallible surfaces may carry a second type — with its own alias, under
these same rules. Cite this divergence in review rather than re-deriving it.

Prior art, not invention: `core_api` (multi-crate workspace, per-crate
`error.rs` with `pub type Result<T>` + flat `Error`, in production for years),
bun (`thiserror`, `#[from]`, `pub type Result<T, E = Error>`), habitat (one
payload-carrying `enum Error`, one alias).

## Functional design (RULE FN-RS)

Design the fallible path as a Result PIPELINE, never as a control-flow tree.

**Fallibility lives in the type, never in a sentinel.** A function that can
fail returns `Result<T>`; a value that can be absent returns `Option<T>`.
Never a bool-plus-out-parameter, never a magic value (-1, empty string, null
UUID), never a logged-and-swallowed error. If the caller cannot see the
failure in the signature, the design is wrong before the body is written.

**Compose with combinators; spend `match` only where arms genuinely differ.**
`?`, `map`, `and_then`, `ok_or_else`, `unwrap_or_else` say "transform on
success, pass failure through" in one line, where a nested match spends five
and gives every arm a chance to swallow the `Err`:

    // WRONG: the failure path is hand-plumbed, twice.
    let config = match load(path) {
        Ok(raw) => match parse(&raw) {
            Ok(c) => c,
            Err(e) => return Err(e.into()),
        },
        Err(e) => return Err(e.into()),
    };

    // RIGHT: one pipeline; `?` and `From` (RULE ERR-RS) do the lifting.
    let config = parse(&load(path)?)?;

A `match` earns its place when the arms carry different LOGIC — not when one
arm is a hand-written `?`.

**Make illegal states unrepresentable; parse, don't validate.** Convert raw
input into a type that can only hold valid states ONCE, at the boundary, and
pass that type inward. Two `Option` fields where exactly one is ever set is an
enum. A `state: String` compared against literals is an enum. A checked-then-
used raw value is a newtype whose constructor returns `Result` — that
constructor is `M-STRONG-TYPES-GUARD`, and picking the type family is
`M-STRONG-TYPES`. What those do not say is the consequence: interior code that
trusts its types needs no defensive re-checks — delete them.

**Expression-orientation: bind once.** `let x = if ... { a } else { b };` and
`let x = match ...` — never a mutable default that later branches overwrite.
A `mut` that exists only to simulate an expression is a bug surface with no
compensating power.

**`unwrap()` / `expect()` are for tests and for invariants already proved.**
`M-PANIC-ON-BUG` settles when a panic is right and `M-PANIC-MESSAGE` that it
must be helpful; the addition here is WHAT the message carries — the proof
("mutex never poisoned: no thread panics while holding it"). In production
code, an `unwrap()` without a written invariant is a `Result` that was never
designed.

Prior art, not invention: `core_api` — a multi-crate production workspace
where every crate with a fallible surface carries its own `error.rs` +
`Result` alias, and the fallible path is written as pipelines throughout
(573 `.map`, 463 `.and_then`, 164 `.ok_or_else` across the workspace). It has
run in production for years; its author attributes the absence of error-path
defects to exactly this design, and the maintenance record bears that out.

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
