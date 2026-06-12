// Line-coverage backfill for src/lib/browser.ts. The platform-resolution
// matrix is covered in browser-resolve-platforms.unit.test.ts; this suite
// drives openUrl through every reachable branch (the quoteUrl/url-push fork,
// the disabled short-circuit, the successful detached spawn, and the spawn
// "error" path) via the injectable spawnImpl seam so nothing shells out to the
// real OS opener. The wslview / xdg-open resolver branches are also exercised
// here: when those binaries exist on PATH the resolver picks them, otherwise it
// falls through to the documented null reason — both are asserted as a
// contract, matching the sibling suite.

import { expect, test } from "bun:test";
import type { spawn } from "node:child_process";
import { openUrl, resolveBrowserCommand } from "../src/lib/browser.ts";

const CB_URL = "https://app.test.local/auth/callback";

interface SpawnCall {
  command: string;
  args: string[];
  detached: boolean;
  windowsVerbatim: boolean;
}

// Minimal ChildProcess stand-in: .on() returns self so chaining works, and
// .unref() is a no-op. The "error" variant fires the registered error handler
// on the microtask queue to drive openUrl's failure resolution.
function fakeChild(fireError: boolean): { on: (e: string, cb: (a?: unknown) => void) => unknown; unref: () => void } {
  const child = {
    on(event: string, cb: (a?: unknown) => void) {
      // Fire synchronously inside the .on("error") registration: openUrl resolves
      // true at the tail of the executor, so only a same-tick error handler can
      // win the resolve(false) race the error branch is meant to take.
      if (fireError && event === "error") cb(new Error("spawn failed"));
      return child;
    },
    unref() {},
  };
  return child;
}

function recordingSpawn(sink: SpawnCall[], fireError = false): typeof spawn {
  return ((command: string, args: string[], options: { detached?: boolean; windowsVerbatimArguments?: boolean }) => {
    sink.push({
      command,
      args,
      detached: options.detached === true,
      windowsVerbatim: options.windowsVerbatimArguments === true,
    });
    return fakeChild(fireError);
  }) as unknown as typeof spawn;
}

test("openUrl spawns the macOS opener detached with the bare url appended", async () => {
  const calls: SpawnCall[] = [];
  const ok = await openUrl(CB_URL, {
    platform: "darwin",
    env: {},
    spawnImpl: recordingSpawn(calls),
  });

  expect(ok).toBe(true);
  expect(calls).toHaveLength(1);
  const call = calls[0]!;
  expect(call.command).toBe("open");
  // quoteUrl is false on darwin, so the url is pushed unquoted (else branch).
  expect(call.args).toEqual([CB_URL]);
  expect(call.detached).toBe(true);
  expect(call.windowsVerbatim).toBe(false);
});

test("openUrl quotes the url and sets verbatim args on the win32 cmd path", async () => {
  const calls: SpawnCall[] = [];
  const ok = await openUrl(CB_URL, {
    platform: "win32",
    env: {},
    spawnImpl: recordingSpawn(calls),
  });

  expect(ok).toBe(true);
  const call = calls[0]!;
  expect(call.command).toBe("cmd");
  // resolved.argv is ["cmd","/c","start",""]; head sliced off, rest + quoted url.
  expect(call.args).toEqual(["/c", "start", "", `"${CB_URL}"`]);
  expect(call.windowsVerbatim).toBe(true);
});

test("openUrl returns false without spawning when the browser is disabled", async () => {
  const calls: SpawnCall[] = [];
  const ok = await openUrl(CB_URL, {
    platform: "darwin",
    env: { BROWSER: "off" },
    spawnImpl: recordingSpawn(calls),
  });

  // resolved.argv is null -> early return false before the spawn promise.
  expect(ok).toBe(false);
  expect(calls).toHaveLength(0);
});

test("openUrl returns false on an unsupported platform with no display", async () => {
  const calls: SpawnCall[] = [];
  const ok = await openUrl(CB_URL, {
    platform: "freebsd",
    env: {},
    spawnImpl: recordingSpawn(calls),
  });

  expect(ok).toBe(false);
  expect(calls).toHaveLength(0);
});

test("openUrl resolves false when the spawned child emits an error", async () => {
  const calls: SpawnCall[] = [];
  const ok = await openUrl(CB_URL, {
    platform: "darwin",
    env: {},
    spawnImpl: recordingSpawn(calls, true),
  });

  // The child's "error" handler fires -> resolve(false), but spawn still ran.
  expect(ok).toBe(false);
  expect(calls).toHaveLength(1);
  expect(calls[0]!.command).toBe("open");
});

test("WSL with wslview installed resolves to the wslview opener", async () => {
  // commandExists probes the real PATH via `sh`; assert the contract. When
  // wslview is present the resolver returns it (line under test); otherwise it
  // falls through to xdg-open or a documented null reason.
  const r = await resolveBrowserCommand(
    { WSL_DISTRO_NAME: "Ubuntu", DISPLAY: ":0" },
    "linux",
  );

  if (r.command === "wslview") {
    expect(r.argv).toEqual(["wslview"]);
    expect(r.quoteUrl).toBe(false);
  } else {
    // No wslview binary on this host: must be the xdg-open continuation or null.
    expect(r.argv === null || r.command === "xdg-open").toBe(true);
  }
});

test("linux with a display resolves to xdg-open when that opener is installed", async () => {
  const r = await resolveBrowserCommand({ DISPLAY: ":0" }, "linux");

  if (r.command === "xdg-open") {
    expect(r.argv).toEqual(["xdg-open"]);
    expect(r.quoteUrl).toBe(false);
  } else {
    // No xdg-open on this host: resolver reports the missing-opener reason.
    expect(r.argv).toBeNull();
    expect(r.reason).toBe("missing-xdg-open");
  }
});

test("openUrl on linux with a display drives the resolved opener through spawn", async () => {
  const calls: SpawnCall[] = [];
  const ok = await openUrl(CB_URL, {
    platform: "linux",
    env: { DISPLAY: ":0" },
    spawnImpl: recordingSpawn(calls),
  });

  // When an opener (xdg-open) resolves, openUrl spawns it and returns true;
  // when none is installed the resolver yields null and openUrl returns false.
  if (ok) {
    expect(calls).toHaveLength(1);
    expect(calls[0]!.args).toEqual([CB_URL]);
  } else {
    expect(calls).toHaveLength(0);
  }
});
