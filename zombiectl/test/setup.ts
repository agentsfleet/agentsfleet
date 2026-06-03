// Bun test preload — runs once before any test file (see bunfig.toml).
import { setDefaultTimeout } from "bun:test";

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
// ZOMBIE_TELEMETRY_DISABLED=0 before invoking the suite.
//
// The spawned-CLI acceptance specs do NOT inherit this — they compose a
// clean child env via `composeEnv`, which injects the same knob directly.
if (process.env.ZOMBIE_TELEMETRY_DISABLED === undefined) {
  process.env.ZOMBIE_TELEMETRY_DISABLED = "1";
}
