// D26 — TTY-priority resolver. Pure function: tests pass the env + TTY
// flag in as a snapshot and assert on the resolved token + source.

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

  test("TTY: ZMB_TOKEN beats ZOMBIE_TOKEN and file", () => {
    expect(
      resolveAuthTokenForCli({
        fileToken: "file-tok",
        env: env({ ZMB_TOKEN: "zmb-tok", ZOMBIE_TOKEN: "zombie-tok" }),
        isTty: true,
      }),
    ).toEqual({ token: "zmb-tok", source: "zmb_env" });
  });

  test("TTY: ZOMBIE_TOKEN beats file when ZMB_TOKEN absent", () => {
    expect(
      resolveAuthTokenForCli({
        fileToken: "file-tok",
        env: env({ ZOMBIE_TOKEN: "zombie-tok" }),
        isTty: true,
      }),
    ).toEqual({ token: "zombie-tok", source: "zombie_env" });
  });

  test("TTY: file token used when no env set", () => {
    expect(
      resolveAuthTokenForCli({ fileToken: "file-tok", env: env({}), isTty: true }),
    ).toEqual({ token: "file-tok", source: "file" });
  });

  test("non-TTY: file beats every env var", () => {
    expect(
      resolveAuthTokenForCli({
        fileToken: "file-tok",
        env: env({ ZMB_TOKEN: "zmb-tok", ZOMBIE_TOKEN: "zombie-tok" }),
        isTty: false,
      }),
    ).toEqual({ token: "file-tok", source: "file" });
  });

  test("non-TTY: ZMB_TOKEN beats ZOMBIE_TOKEN when no file", () => {
    expect(
      resolveAuthTokenForCli({
        fileToken: null,
        env: env({ ZMB_TOKEN: "zmb-tok", ZOMBIE_TOKEN: "zombie-tok" }),
        isTty: false,
      }),
    ).toEqual({ token: "zmb-tok", source: "zmb_env" });
  });

  test("non-TTY: ZOMBIE_TOKEN used when only it is set", () => {
    expect(
      resolveAuthTokenForCli({
        fileToken: null,
        env: env({ ZOMBIE_TOKEN: "zombie-tok" }),
        isTty: false,
      }),
    ).toEqual({ token: "zombie-tok", source: "zombie_env" });
  });

  test("whitespace-only env values are treated as unset", () => {
    expect(
      resolveAuthTokenForCli({
        fileToken: null,
        env: env({ ZMB_TOKEN: "   ", ZOMBIE_TOKEN: "real" }),
        isTty: true,
      }),
    ).toEqual({ token: "real", source: "zombie_env" });
  });

  test("env values are trimmed before being returned", () => {
    expect(
      resolveAuthTokenForCli({
        fileToken: null,
        env: env({ ZMB_TOKEN: "  padded  " }),
        isTty: true,
      }),
    ).toEqual({ token: "padded", source: "zmb_env" });
  });

  test("empty-string file token equivalent to unset", () => {
    expect(
      resolveAuthTokenForCli({
        fileToken: "",
        env: env({ ZOMBIE_TOKEN: "fallback" }),
        isTty: false,
      }),
    ).toEqual({ token: "fallback", source: "zombie_env" });
  });
});
