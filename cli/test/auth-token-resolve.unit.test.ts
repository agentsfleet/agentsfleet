// TTY-priority resolver. Pure function: tests pass the env + TTY flag in
// as a snapshot and assert on the resolved token + source. The two sources
// are the on-disk file token and the AGENTSFLEET_TOKEN env var.

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

  test("TTY: AGENTSFLEET_TOKEN env beats a stale file token", () => {
    expect(
      resolveAuthTokenForCli({
        fileToken: "file-tok",
        env: env({ AGENTSFLEET_TOKEN: "agent-tok" }),
        isTty: true,
      }),
    ).toEqual({ token: "agent-tok", source: "agent_env" });
  });

  test("TTY: file token used when AGENTSFLEET_TOKEN unset", () => {
    expect(
      resolveAuthTokenForCli({ fileToken: "file-tok", env: env({}), isTty: true }),
    ).toEqual({ token: "file-tok", source: "file" });
  });

  test("non-TTY: file token beats AGENTSFLEET_TOKEN env", () => {
    expect(
      resolveAuthTokenForCli({
        fileToken: "file-tok",
        env: env({ AGENTSFLEET_TOKEN: "agent-tok" }),
        isTty: false,
      }),
    ).toEqual({ token: "file-tok", source: "file" });
  });

  test("non-TTY: AGENTSFLEET_TOKEN used when no file", () => {
    expect(
      resolveAuthTokenForCli({
        fileToken: null,
        env: env({ AGENTSFLEET_TOKEN: "agent-tok" }),
        isTty: false,
      }),
    ).toEqual({ token: "agent-tok", source: "agent_env" });
  });

  test("whitespace-only AGENTSFLEET_TOKEN is treated as unset (falls to file)", () => {
    expect(
      resolveAuthTokenForCli({
        fileToken: "file-tok",
        env: env({ AGENTSFLEET_TOKEN: "   " }),
        isTty: true,
      }),
    ).toEqual({ token: "file-tok", source: "file" });
  });

  test("AGENTSFLEET_TOKEN env value is trimmed before being returned", () => {
    expect(
      resolveAuthTokenForCli({
        fileToken: null,
        env: env({ AGENTSFLEET_TOKEN: "  padded  " }),
        isTty: true,
      }),
    ).toEqual({ token: "padded", source: "agent_env" });
  });

  test("empty-string file token equivalent to unset (falls to AGENTSFLEET_TOKEN)", () => {
    expect(
      resolveAuthTokenForCli({
        fileToken: "",
        env: env({ AGENTSFLEET_TOKEN: "fallback" }),
        isTty: false,
      }),
    ).toEqual({ token: "fallback", source: "agent_env" });
  });
});
