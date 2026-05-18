// browser.layer.ts unit tests. The layer wraps lib/browser.ts:openUrl,
// which already owns the BROWSER/DISPLAY/SSH/WSL precedence + spawn.
// We exercise the layer's contract: it provides a Browser service that
// returns a boolean from `open` and never throws. Behaviour under
// blocked environments is asserted via BROWSER=false (the most
// portable opt-out — works on every platform without an actual spawn).

import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { Effect } from "effect";
import { Browser } from "../../src/services/browser.service.ts";
import { browserLayer } from "../../src/services/browser.layer.ts";

const savedBrowser = process.env.BROWSER;
const savedDisplay = process.env.DISPLAY;
const savedWaylandDisplay = process.env.WAYLAND_DISPLAY;
const savedSshClient = process.env.SSH_CLIENT;
const savedSshTty = process.env.SSH_TTY;
const savedSshConnection = process.env.SSH_CONNECTION;

beforeEach(() => {
  delete process.env.DISPLAY;
  delete process.env.WAYLAND_DISPLAY;
  delete process.env.SSH_CLIENT;
  delete process.env.SSH_TTY;
  delete process.env.SSH_CONNECTION;
});

afterEach(() => {
  if (savedBrowser === undefined) delete process.env.BROWSER;
  else process.env.BROWSER = savedBrowser;
  if (savedDisplay === undefined) delete process.env.DISPLAY;
  else process.env.DISPLAY = savedDisplay;
  if (savedWaylandDisplay === undefined) delete process.env.WAYLAND_DISPLAY;
  else process.env.WAYLAND_DISPLAY = savedWaylandDisplay;
  if (savedSshClient === undefined) delete process.env.SSH_CLIENT;
  else process.env.SSH_CLIENT = savedSshClient;
  if (savedSshTty === undefined) delete process.env.SSH_TTY;
  else process.env.SSH_TTY = savedSshTty;
  if (savedSshConnection === undefined) delete process.env.SSH_CONNECTION;
  else process.env.SSH_CONNECTION = savedSshConnection;
});

const runOpen = (url: string): Promise<boolean> =>
  Effect.runPromise(
    Effect.gen(function* () {
      const browser = yield* Browser;
      return yield* browser.open(url);
    }).pipe(Effect.provide(browserLayer)),
  );

describe("browserLayer", () => {
  test("provides a Browser service exposing `open`", () => {
    const program = Effect.gen(function* () {
      const browser = yield* Browser;
      expect(typeof browser.open).toBe("function");
    }).pipe(Effect.provide(browserLayer));
    return Effect.runPromise(program);
  });

  test("BROWSER=false returns false without spawning", async () => {
    process.env.BROWSER = "false";
    const result = await runOpen("https://example.test/login");
    expect(result).toBe(false);
  });

  test("BROWSER=off returns false", async () => {
    process.env.BROWSER = "off";
    const result = await runOpen("https://example.test/login");
    expect(result).toBe(false);
  });

  test("BROWSER=none returns false", async () => {
    process.env.BROWSER = "none";
    const result = await runOpen("https://example.test/login");
    expect(result).toBe(false);
  });

  test("SSH session with no DISPLAY returns false on linux", async () => {
    if (process.platform !== "linux") return;
    delete process.env.BROWSER;
    process.env.SSH_CLIENT = "1.2.3.4 4321 22";
    const result = await runOpen("https://example.test/login");
    expect(result).toBe(false);
  });

  test("open never throws — error channel is `never`", async () => {
    process.env.BROWSER = "false";
    const result = await runOpen("not-a-url");
    expect(typeof result).toBe("boolean");
  });
});
