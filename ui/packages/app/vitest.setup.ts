import { afterEach, vi } from "vitest";
import { configure } from "@testing-library/dom";

// Unit tests declare their backend origin explicitly — lib/api/client.ts
// throws on an unset NEXT_PUBLIC_API_URL instead of falling back (a silent
// fallback once pointed env-less worktrees at the shared dev API). `??=`
// keeps a caller-provided value intact.
process.env.NEXT_PUBLIC_API_URL ??= "https://api-test.agentsfleet.net";

/// Async ceiling for waitFor/findBy* across the suite.
const ASYNC_UTIL_TIMEOUT_MS = 5_000;

// No unit test may touch a real network. Without this, a fetch that a test does
// not explicitly mock resolves against the dev-API origin (localhost:3000),
// gets ECONNREFUSED, and hangs to the 10s test timeout under parallel load — a
// flaky cascade: the hung test skips its afterEach cleanup, leaking its rendered
// DOM into the next file's tests as "Found multiple elements". Install a benign
// default ONCE per file (setup files run before the test file's own module code
// and its hooks, so a test that mocks `global.fetch` at module scope or in a
// hook still overrides this). The default just stops an unmocked — usually
// incidental: analytics beacon, route prefetch — request from dialing a socket.
// Set at module top-level, NOT in beforeEach: a per-test reset would clobber the
// module-scope fetch mocks many suites install once.
//
// Escape hatch: a real-transport integration suite (lib/api/retry.integration
// .test.ts drives retry/backoff over a live localhost socket) restores the real
// implementation via `globalThis.__realFetch`, captured here before the swap.
(globalThis as { __realFetch?: typeof fetch }).__realFetch = globalThis.fetch;
globalThis.fetch = (async () =>
  new Response("{}", {
    status: 200,
    headers: { "content-type": "application/json" },
  })) as unknown as typeof globalThis.fetch;

// Reset to real timers after every test so a previous test's
// `vi.useFakeTimers()` cannot bleed into the next file's `waitFor`
// (which itself uses setTimeout internally and hangs if mocked).
afterEach(() => {
  vi.useRealTimers();
});

// testing-library's 1s async ceiling is a quiet-machine number. Under the whole
// suite — 235 files in parallel — mounting a Radix dialog and settling its focus
// guard routinely costs longer than that, which surfaced as FleetLibrariesView
// failing to find a dialog it had rendered correctly. Raising the ceiling weakens
// no assertion: a test that would fail still fails, it just stops failing for
// having been run on a loaded machine. It costs wall time only on runs that are
// already failing.
configure({ asyncUtilTimeout: ASYNC_UTIL_TIMEOUT_MS });
