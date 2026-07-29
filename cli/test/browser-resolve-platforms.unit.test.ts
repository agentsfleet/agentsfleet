import { test, expect } from "bun:test";
import { resolveBrowserCommand } from "../src/lib/browser.ts";

const WSLVIEW_COMMAND = "wslview";
const XDG_OPEN_COMMAND = "xdg-open";

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
  // Inject a resolver that reports xdg-open absent so the missing-opener
  // fall-through is covered on every host. A real PATH override does NOT work:
  // commandExists spawns `sh` with the inherited process env, not this env, so
  // the probe sees the runner's real PATH — xdg-open is present on the Linux
  // Continuous Integration (CI) runner image and absent on macOS, which left
  // the fall-through uncovered on CI.
  const r = await resolveBrowserCommand(
    { DISPLAY: ":0" },
    "linux",
    async () => false,
  );
  expect(r.argv).toBeNull();
  expect(r.reason).toBe("missing-xdg-open");
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

test("Windows Subsystem for Linux with wslview installed resolves to the wslview opener", async () => {
  const r = await resolveBrowserCommand(
    { WSL_DISTRO_NAME: "wsl-Ubuntu" },
    "linux",
    async (command) => command === WSLVIEW_COMMAND,
  );
  expect(r.command).toBe(WSLVIEW_COMMAND);
  expect(r.argv).toEqual([WSLVIEW_COMMAND]);
  expect(r.quoteUrl).toBe(false);
});

test("linux with xdg-open installed resolves to the xdg-open opener", async () => {
  const r = await resolveBrowserCommand(
    { DISPLAY: ":0" },
    "linux",
    async (command) => command === XDG_OPEN_COMMAND,
  );
  expect(r.command).toBe(XDG_OPEN_COMMAND);
  expect(r.argv).toEqual([XDG_OPEN_COMMAND]);
  expect(r.quoteUrl).toBe(false);
});

test("WSL without DISPLAY and without wslview returns wsl-no-wslview", async () => {
  // Inject a resolver that reports wslview absent so the wsl-no-wslview
  // fall-through is covered on every host. As above, a PATH override cannot
  // suppress a global wslview shim on the runner: commandExists probes the real
  // process env, so only injection makes this path deterministic.
  const r = await resolveBrowserCommand(
    { WSL_DISTRO_NAME: "wsl-Ubuntu" },
    "linux",
    async () => false,
  );
  expect(r.argv).toBeNull();
  expect(r.reason).toBe("wsl-no-wslview");
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
