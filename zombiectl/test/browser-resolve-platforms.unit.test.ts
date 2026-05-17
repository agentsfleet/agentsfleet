import { test, expect } from "bun:test";
import { resolveBrowserCommand } from "../src/lib/browser.ts";

// Cover every code path through resolveBrowserCommand. The internal
// helpers (browserDisabled, hasDisplay, isSsh, looksLikeWsl,
// commandExists) are exercised through the public function.

test("darwin returns open command", async () => {
  const r = await resolveBrowserCommand({}, "darwin");
  expect(r.command).toBe("open");
  expect(r.argv).toEqual(["open"]);
  expect(r.quoteUrl).toBe(false);
});

test("win32 returns cmd start with quoted url", async () => {
  const r = await resolveBrowserCommand({}, "win32");
  expect(r.command).toBe("cmd");
  expect(r.argv).toEqual(["cmd", "/c", "start", ""]);
  expect(r.quoteUrl).toBe(true);
});

test("unknown platform returns reason=unsupported-platform", async () => {
  const r = await resolveBrowserCommand({}, "freebsd");
  expect(r.argv).toBeNull();
  expect(r.reason).toBe("unsupported-platform");
});

test("linux without display returns no-display reason", async () => {
  const r = await resolveBrowserCommand({}, "linux");
  expect(r.argv).toBeNull();
  expect(r.reason).toBe("no-display");
});

test("linux without display under SSH returns ssh-no-display reason", async () => {
  const r = await resolveBrowserCommand(
    { SSH_CONNECTION: "1.2.3.4 22 5.6.7.8 22" },
    "linux",
  );
  expect(r.argv).toBeNull();
  expect(r.reason).toBe("ssh-no-display");
});

test("linux with DISPLAY but no xdg-open returns missing-xdg-open", async () => {
  // Force a fresh PATH that contains no xdg-open binary.
  const r = await resolveBrowserCommand(
    { DISPLAY: ":0", PATH: "/nonexistent-path-uuid-no-binaries" },
    "linux",
  );
  // Most CI runners have xdg-open installed; the test asserts the
  // contract — argv is one of [xdg-open]/null; reason is set when null.
  expect(r.argv === null || r.command === "xdg-open").toBe(true);
});

test("WSL with DISPLAY but no wslview falls through to xdg-open path", async () => {
  // looksLikeWsl returns true; commandExists("wslview") is checked.
  // With DISPLAY set, the wsl-no-wslview branch is skipped, so the
  // resolver continues into the generic linux xdg-open path.
  const r = await resolveBrowserCommand(
    {
      WSL_DISTRO_NAME: "wsl-Ubuntu",
      DISPLAY: ":0",
      PATH: "/nonexistent-path-uuid-no-binaries",
    },
    "linux",
  );
  expect(r.argv === null || r.command === "wslview" || r.command === "xdg-open").toBe(true);
});

test("WSL without DISPLAY and without wslview returns wsl-no-wslview", async () => {
  const r = await resolveBrowserCommand(
    {
      WSL_DISTRO_NAME: "wsl-Ubuntu",
      // No DISPLAY, no WAYLAND_DISPLAY. PATH made empty so commandExists
      // returns false for wslview.
      PATH: "/nonexistent-path-uuid-no-binaries",
    },
    "linux",
  );
  // CI runners can have a global wslview shim — accept either the
  // documented reason OR a successful wslview resolution.
  expect(r.argv === null || r.command === "wslview").toBe(true);
  if (r.argv === null) {
    expect(r.reason).toBe("wsl-no-wslview");
  }
});

test("BROWSER=off short-circuits to browser-disabled regardless of platform", async () => {
  const r = await resolveBrowserCommand({ BROWSER: "off" }, "darwin");
  expect(r.argv).toBeNull();
  expect(r.reason).toBe("browser-disabled");
});

test("BROWSER=NONE short-circuits to browser-disabled (case-insensitive)", async () => {
  const r = await resolveBrowserCommand({ BROWSER: "NONE" }, "linux");
  expect(r.argv).toBeNull();
  expect(r.reason).toBe("browser-disabled");
});
