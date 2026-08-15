// Bun test preload — runs once before any test file (see bunfig.toml).
import { setDefaultTimeout } from "bun:test";

import { makeEmptyStateDirSync } from "./acceptance/fixtures/state-dir.ts";

// Acceptance specs spawn the real CLI as a child process and await its
// exit. Under `bun test --coverage` the parent runner is instrumented,
// so spawn-and-await occasionally exceeds bun's 5000ms default per-test
// timeout — a flaky tail unrelated to the code under test. Raise the
// default to give subprocess tests headroom; per-test explicit timeouts
// still override, and telemetry-off (below) removes the real 5s hang.
const SPAWN_TEST_TIMEOUT_MS = 15_000;
setDefaultTimeout(SPAWN_TEST_TIMEOUT_MS);

// The CLI's telemetry flush reaches PostHog over the network and hangs
// ~5s when the runner is offline, which times out the in-process
// `runCli` integration tests (they read `process.env` directly). Default
// the whole runner to telemetry-off so the suite is hermetic under any
// runner. Tests that exercise telemetry consent manage this env var
// themselves (save + delete in beforeEach, restore in afterEach), so the
// default is invisible to them. A developer may still opt in by exporting
// AGENTSFLEET_TELEMETRY_DISABLED=0 before invoking the suite.
//
// The spawned-CLI acceptance specs do NOT inherit this — they compose a
// clean child env via `composeEnv`, which injects the same knob directly.
if (process.env.AGENTSFLEET_TELEMETRY_DISABLED === undefined) {
  process.env.AGENTSFLEET_TELEMETRY_DISABLED = "1";
}

// The store resolves from the environment `runCli` is handed (an injected
// `io.env` reaches it; see src/lib/state.ts). This process-env default exists
// for the fixtures that still seed state THROUGH the process environment —
// `withFreshStateDir` / `withAuthedStateDir` swap it per case and tests bridge
// it into `io.env` via `cliEnv()` — so with nothing set, logged-out
// against an empty directory is the baseline instead of whatever real login
// sits in `~/.config/agentsfleet`. A test wanting stored state sets its own
// (see `makeStubbedStateDir`), and an already-exported value is left alone.
//
// The spawned-CLI acceptance specs do NOT inherit this — they compose a clean
// child env via `composeEnv`, so they inject the same knob directly.
if (process.env.AGENTSFLEET_STATE_DIR === undefined) {
  process.env.AGENTSFLEET_STATE_DIR = makeEmptyStateDirSync();
}
