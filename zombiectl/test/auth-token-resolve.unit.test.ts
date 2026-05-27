// TTY-priority resolver. Pure function: tests pass the env + TTY flag in
// as a snapshot and assert on the resolved token + source. The two sources
// are the on-disk file token and the ZOMBIE_TOKEN env var.

import { describe, expect, test } from "bun:test";
import { resolveAuthTokenForCli } from "../src/program/auth-token.ts";

const env = (record: Record<string, string>): NodeJS.ProcessEnv => record;

describe("resolveAuthTokenForCli", () => {
  test("nothing set → source: none", () => {
    expect(resolveAuthTokenForCli({ fileToken: null, env: env({}), isTty: true })).toEqual({
      token: null,
      source: "none",
    });
  });

  test("TTY: ZOMBIE_TOKEN env beats a stale file token", () => {
    expect(
      resolveAuthTokenForCli({
        fileToken: "file-tok",
        env: env({ ZOMBIE_TOKEN: "zombie-tok" }),
        isTty: true,
      }),
    ).toEqual({ token: "zombie-tok", source: "zombie_env" });
  });

  test("TTY: file token used when ZOMBIE_TOKEN unset", () => {
    expect(
      resolveAuthTokenForCli({ fileToken: "file-tok", env: env({}), isTty: true }),
    ).toEqual({ token: "file-tok", source: "file" });
  });

  test("non-TTY: file token beats ZOMBIE_TOKEN env", () => {
    expect(
      resolveAuthTokenForCli({
        fileToken: "file-tok",
        env: env({ ZOMBIE_TOKEN: "zombie-tok" }),
        isTty: false,
      }),
    ).toEqual({ token: "file-tok", source: "file" });
  });

  test("non-TTY: ZOMBIE_TOKEN used when no file", () => {
    expect(
      resolveAuthTokenForCli({
        fileToken: null,
        env: env({ ZOMBIE_TOKEN: "zombie-tok" }),
        isTty: false,
      }),
    ).toEqual({ token: "zombie-tok", source: "zombie_env" });
  });

  test("whitespace-only ZOMBIE_TOKEN is treated as unset (falls to file)", () => {
    expect(
      resolveAuthTokenForCli({
        fileToken: "file-tok",
        env: env({ ZOMBIE_TOKEN: "   " }),
        isTty: true,
      }),
    ).toEqual({ token: "file-tok", source: "file" });
  });

  test("ZOMBIE_TOKEN env value is trimmed before being returned", () => {
    expect(
      resolveAuthTokenForCli({
        fileToken: null,
        env: env({ ZOMBIE_TOKEN: "  padded  " }),
        isTty: true,
      }),
    ).toEqual({ token: "padded", source: "zombie_env" });
  });

  test("empty-string file token equivalent to unset (falls to ZOMBIE_TOKEN)", () => {
    expect(
      resolveAuthTokenForCli({
        fileToken: "",
        env: env({ ZOMBIE_TOKEN: "fallback" }),
        isTty: false,
      }),
    ).toEqual({ token: "fallback", source: "zombie_env" });
  });
});
