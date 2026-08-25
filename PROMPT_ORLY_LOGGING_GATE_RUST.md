# Prompt — close the Rust logging gate's two carve-out gaps in orly

Ephemeral. Paste the block below to a fresh agent working on the
`@agentsfleet/orly` engine. Delete once the fix ships and this repository has
re-run `orly update`.

---

You are fixing a gate defect in the `@agentsfleet/orly` engine — the package
that materialises `audits/`, `dispatch/` and `AGENTS.orly.md` into consuming
repositories. The defect is in `audits/logging.sh`, the LOGGING gate.

**Do not fix this by editing `audits/logging.sh` inside a consuming repository.**
That file is orly-managed: `orly update` overwrites it and `orly doctor` reports
the edit as drift. The fix belongs in the engine, and ships as `0.7.1`.

## What happened

`orly 0.7.0` extended the LOGGING gate to Rust — `logging.sh` now scans
`rustd/**/*.rs` alongside `src/**/*.zig`. That was the right change and it
immediately found 65 real violations in the `agentsfleet` Rust daemon, all now
fixed in that repository.

It also produced **6 false positives that cannot be fixed in the consuming
repository**, because the code the gate objects to is correct. They are all
`rust-direct` hits — "direct Rust diagnostic macro in non-test source":

```
rustd/crates/afd_api/build.rs:37       println!("cargo:rerun-if-changed={GIT_HEAD}")
rustd/crates/afd_api/build.rs:38       println!("cargo:rerun-if-env-changed={KNOB}")
rustd/crates/afd_api/build.rs:50       println!("cargo:rustc-env={KNOB}={}", ...)
rustd/crates/afd_api/build.rs:82       println!("cargo:rustc-env={KNOB}={commit}{suffix}")
rustd/crates/agentsfleetd/src/banner.rs:52   println!(...)   // startup banner
rustd/crates/agentsfleetd/src/fatal.rs:128   eprintln!(...)  // fatal renderer
```

The four `build.rs` lines are not a style choice: **stdout is cargo's IPC
channel.** `cargo::rustc-env=` is how a build script sets an environment
variable for the crate it builds. Converting them to `tracing` does not make
the logging cleaner — it breaks the build, silently, because the variable is
never set and the crate compiles with a stale or absent value.

The other two are a design the gate has no vocabulary for. `banner.rs` prints
the startup banner: the program's ANSWER to an operator's command, read by
humans and scripts on stdout, where a log record would be wrong. `fatal.rs::die`
renders a fatal to stderr and is documented in-source as "the only thing in this
crate that does" — it must work when the tracing subscriber does not exist yet.

**A seventh hit was a real finding and is already fixed**, which is worth
knowing before you assume the rest are noise. `cli.rs` printed the `agentsfleetd
migrate` summary to stdout while every step that produced it —
`migrate_conn_acquired`, `migrate_lock_acquired`, `migrate_refused_schema_ahead`
— already emitted a structured event. The summary was the one part of that path
that could not be queried beside the events it summarised. It is now
`migrate_completed`; the command's contract is its exit status, which is what
the integration lane asserts and which did not change.

## The fix, in two parts

Both extend a mechanism that **already exists in this pack**. Neither invents
one. Do both; part 1 alone leaves the harder half unaddressed.

### Part 1 — bring the Rust carve-out up to the Zig carve-out's thoroughness

Same file, same gate, two very different lists:

| helper | patterns |
|---|---|
| `is_test_zig` | `*_test.zig`, `*_test_harness.zig`, `*_test_helper.zig`, `*/tests/*`, `test_harness.zig`, `test_helper.zig` — **6** |
| `is_test_rust_path` | `*/tests/*`, `*/benches/*` — **2** |

The Rust helper landed later and never got the same care. Add the non-runtime
Rust paths it is missing:

- `build.rs` — a cargo build script, at any crate root. Not runtime source; its
  stdout is a protocol.
- `*/examples/*` — compiled, but demonstration rather than product.

Consider renaming the helper while you are there. `is_test_rust_path` is
already answering "is this non-runtime source", which is a wider question than
its name admits, and the name is what made `build.rs` easy to overlook.

### Part 2 — give LOGGING the inline escape hatch its sibling gate already has

This is the reusable half, and the one that matters beyond this repository.

`audits/ufs.sh:22` carries one:

> Carve-out: any `// pin test: literal is the contract` comment on or above the
> line

and `ufs.sh:411` advertises it in the failure output, so an author who trips
the gate is told how to declare an intentional case. `logging.sh` has no
equivalent, so a file where writing to a stream directly IS the design has no
way to say so and no way to be reviewed for saying so.

Add a reason-carrying annotation the Rust scanner honours on, or immediately
above, the offending line. Match `ufs.sh`'s shape and its advertising:

```rust
// logging: stdout is this command's answer, not a log record
println!("{}", render(Rendering::of_stdout(), version, roles, pid));
```

**The reason text must be required and non-empty.** A bare marker becomes a
silent mute that spreads by copy-paste; a required reason is reviewable in a
diff and is the same principle the consuming workspace already enforces with
`allow_attributes_without_reason = "deny"` and `#[expect(..., reason = ...)]`.

Apply the hatch to the Zig half too if it lacks one — the asymmetry in part 1
suggests the two halves have drifted, and this is the moment to check rather
than to widen the gap.

## Acceptance

1. In a checkout of `agentsfleet` on `feat/m177-runner-control-plane-parity`,
   after `orly update`:
   - `bash audits/logging.sh --all` reports `rust-direct=0` with the four
     `build.rs` lines untouched.
   - The two annotated sites (`banner.rs`, `fatal.rs`) pass **only** with their
     reason comment present; deleting the reason re-fails them. Prove both
     directions.
2. `make harness-verify-all` passes, which it does not today.
3. `orly doctor` is green — the consuming repository holds no local edit to
   `audits/logging.sh`.
4. The gate still catches a real violation: add a `println!` with no annotation
   to a non-test Rust source file and confirm it fails.
5. `dispatch/write_any.md`'s Logging Gate section and `docs/LOGGING_STANDARD.md`
   §8A both document the annotation, including that the reason is mandatory.
   §8A's anti-pattern table is where an author will look for it.

## What NOT to do

- Do not add an allowlist of specific file paths from the `agentsfleet`
  repository into the engine. `error-codes.sh` does carry one and it is a
  liability, not a precedent to copy: it names consuming-repository files
  inside a general-purpose engine.
- Do not relax the `rust-missing-event` rule. It found 65 genuine problems on
  its first run and every one was worth fixing.
- Do not exempt `src/` or `rustd/` wholesale.
