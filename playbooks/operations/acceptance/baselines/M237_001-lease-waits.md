# M237_001 lease waits: before and after the build moves beside the model

Branch: `feat/m237-sandbox-lease-path`.
Comparison revision: `2f8c878c09e9c4048da1724385ec58746d348ed4`.
Indy asked on Nov 02, 2026 for the measurement to run locally through the make targets, not on the dev host.

## Where the figures come from

| Source | Host | Kernel | What it measures |
|---|---|---|---|
| `make test-runner-kernel`, `test_start_budgets_with_four_leases` | OrbStack machine `afr-kernel`, Debian 13.7 | `7.2.12-orbstack-00394-gbecdd6130068` | Real `BubblewrapEngine` starts: executor-ready and first command, 100 leases, 4 at once |
| `cargo test -p afr_supervisor` lease tests, paused clock | macOS, fake engine | none | Where each wait sits on the lease's path, exactly, with set delays |

The dev host's own logs held three `sandbox_prepare_completed` events in the seven days before Nov 02, too few for a p95.
The OrbStack lane runs OrbStack's kernel, not Debian's `6.12`, so its figures size the waits rather than predict the dev host's.

## Sandbox start, measured on the kernel lane

Measured at `937c8d34d`, where §1 only adds timers; the engine is the comparison revision's.

| Cache | Executor ready p50 | p95 | p99 | First command p50 | p95 | p99 |
|---|---|---|---|---|---|---|
| cold | 100.8 ms | 321.6 ms | 442.7 ms | 5.2 ms | 12.6 ms | 13.8 ms |
| warm | 95.7 ms | 119.3 ms | 122.3 ms | 1.4 ms | 2.0 ms | 2.1 ms |

Host staging took 1.98 s cold and 2.32 s warm at p50 of three, once per toolbox, outside any lease.

## The lease's path, before and after

Before, the first model request waited for the egress bind, the prepare and every checkout, in that order.
After, it waits for the egress bind alone; the prepare and the checkout run beside it.
A sandbox tool call waits only for what is left of the build, and `lease_completed` names that wait `sandbox_wait_ms`.

| Wait | Before | After | Proof |
|---|---|---|---|
| First model request behind the prepare | the whole prepare, about 100 ms warm and 322 ms cold at p95 | none | `test_first_model_call_starts_before_the_sandbox_is_ready`: with a 500 ms prepare, the model began 500 ms before the sandbox was ready |
| First model request behind the checkout | every checkout's token mint and copy | none | `test_an_answer_mid_checkout_holds_the_whole_sandbox` |
| A sandbox tool call | the build had already finished | what is left of the build, logged as `sandbox_wait_ms` | `test_sandbox_tool_waits_for_the_build`: `sandbox_wait_ms=500` |
| The report behind the disk's release | unmount and image removal | none | `test_a_lease_returns_only_after_its_release`: a 3 s release left `report_ms` under 3 s, and the lease still returned after it |

`lease_completed` now carries `egress_ms`, `prepare_ms`, `checkout_ms` and `sandbox_wait_ms` beside `sandbox_ms`.
After the change ships, those fields give the production split this file could not take from three dev-host leases.
