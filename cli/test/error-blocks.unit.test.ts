// Every failure prints one aligned block in text mode, the same envelope it
// always printed under `--json`, and requested help is left alone.

import { describe, expect, test } from "bun:test";
import path from "node:path";
import { Writable } from "node:stream";

import { runCli } from "../src/cli.ts";
import { BLOCK_VALUE_COLUMN, PLAIN_STYLE, renderErrorBlock, splitFix } from "../src/output/error-block.ts";
import { cliEnv, withFreshStateDir } from "./helpers-cli-state.ts";

const FLEET_ID = "0192a3b4-c5d6-7e8f-9012-345678901234";
const WORKSPACE_ID = "0192a3b4-c5d6-7e8f-9012-345678901230";
const API_URL = "http://stub.test";
const ANSI = /\u001B\[[0-9;]*m/g;
const LABEL_LINE = /^ {2}(code|usage|did you mean|suggestion|request) +/;
// Values start at column 17, the width the spec pins, not one the code reports.
const VALUE_INDEX = 16;

type Reply = "forbidden" | "unreachable";

interface Failure {
  readonly name: string;
  readonly argv: ReadonlyArray<string>;
  readonly signedIn: boolean;
  readonly reply: Reply;
}

// One of each kind the block renders, all reachable without a live server.
const FAILURES: ReadonlyArray<Failure> = [
  { name: "server error", argv: ["fleet", "show", FLEET_ID], signedIn: true, reply: "forbidden" },
  { name: "network error", argv: ["fleet", "show", FLEET_ID], signedIn: true, reply: "unreachable" },
  { name: "guard refusal", argv: ["fleet", "list"], signedIn: false, reply: "forbidden" },
  { name: "usage error", argv: ["fleet", "archive", "abc"], signedIn: true, reply: "forbidden" },
  { name: "unknown command", argv: ["workspace", "lsit"], signedIn: false, reply: "forbidden" },
  { name: "invalid completions shell", argv: ["--completions", "tcsh"], signedIn: false, reply: "forbidden" },
];

const terminal = (columns: number) => {
  let text = "";
  const stream = new Writable({ write(chunk, _encoding, done) { text += String(chunk); done(); } });
  return { stream: Object.assign(stream, { isTTY: true, columns }), read: () => text.replace(ANSI, "") };
};

const forbidden = () =>
  new Response(
    JSON.stringify({ error_code: "UZ-AUTH-022", user_message: "Missing fleet read scope", request_id: "req_block_1" }),
    { status: 403, headers: { "content-type": "application/json" } },
  );

const run = (failure: Failure, options: { json: boolean; columns: number }) =>
  withFreshStateDir(async (dir) => {
    await Bun.write(
      path.join(dir, "workspaces.json"),
      JSON.stringify({ current_workspace_id: WORKSPACE_ID, items: [{ workspace_id: WORKSPACE_ID }] }),
    );
    const fetchImpl = async () => {
      if (failure.reply === "unreachable") throw new TypeError("fetch failed");
      return forbidden();
    };
    const out = terminal(options.columns);
    const err = terminal(options.columns);
    const code = await runCli([...(options.json ? ["--json"] : []), ...failure.argv], {
      stdout: out.stream,
      stderr: err.stream,
      env: cliEnv({
        NO_COLOR: "1",
        AGENTSFLEET_API_URL: API_URL,
        ...(failure.signedIn ? { AGENTSFLEET_API_KEY: "agt_t_blocks" } : {}),
      }),
      fetchImpl: fetchImpl as unknown as typeof fetch,
    });
    return { code, out: out.read(), err: err.read() };
  });

describe("error_blocks_align_labels_and_wrap", () => {
  for (const failure of FAILURES) {
    for (const columns of [60, 120]) {
      test(`${failure.name} at ${columns} columns`, async () => {
        const result = await run(failure, { json: false, columns });
        expect(result.out).toBe("");
        expect(result.err).not.toContain("DESCRIPTION");
        const [head, ...rest] = result.err.trimEnd().split("\n");
        expect(head?.startsWith("✕ error: ")).toBe(true);
        expect(rest.length).toBeGreaterThan(0);
        for (const line of rest) {
          // Either a label in the margin or a blank margin (a wrapped
          // continuation); in both, the value begins at the value column.
          const margin = line.slice(0, VALUE_INDEX);
          expect(LABEL_LINE.test(line) || margin.trim() === "").toBe(true);
          expect(line.charAt(VALUE_INDEX)).not.toBe(" ");
          expect(line.length).toBeGreaterThan(VALUE_INDEX);
          // Wrapping keeps lines inside the width, unless one word is wider.
          const longestWord = Math.max(...line.slice(VALUE_INDEX).split(" ").map((word) => word.length));
          expect(line.length <= columns || longestWord > columns - VALUE_INDEX).toBe(true);
        }
      });
    }
  }

  test("a server error carries its code, fix and request id", async () => {
    const result = await run(FAILURES[0]!, { json: false, columns: 120 });
    expect(result.err).toContain("✕ error: Missing fleet read scope");
    expect(result.err).toMatch(/^ {2}code {10}UZ-AUTH-022$/m);
    expect(result.err).toMatch(/^ {2}request {7}req_block_1$/m);
  });

  test("an unknown command names the near miss on its own line", async () => {
    const result = await run(FAILURES[4]!, { json: false, columns: 120 });
    expect(result.err).toMatch(/^ {2}did you mean {2}list$/m);
    expect(result.err).not.toContain("Did you mean this?");
  });

  test("a leaf rejection carries its usage line and the leaf hint", async () => {
    const result = await run({ name: "leaf", argv: ["fleet", "show"], signedIn: true, reply: "forbidden" }, { json: false, columns: 120 });
    expect(result.code).toBe(4);
    expect(result.err).toMatch(/^ {2}usage {9}agentsfleet fleet show \[options\] <fleet_id>$/m);
    expect(result.err).toMatch(/^ {2}suggestion {4}run `agentsfleet fleet show --help` for its arguments and flags$/m);
  });

  test("a choice error lists its values in words", async () => {
    const result = await run(FAILURES[5]!, { json: false, columns: 120 });
    expect(result.err).toContain("✕ error: invalid --completions: expected one of bash, zsh, fish, powershell, sh, pwsh (got \"tcsh\")");
  });

  test("a root typo is one block with no near miss to offer", async () => {
    const result = await run({ name: "root typo", argv: ["lsit"], signedIn: false, reply: "forbidden" }, { json: false, columns: 120 });
    expect(result.code).toBe(4);
    expect(result.out).toBe("");
    expect(result.err).toBe(
      '✕ error: Unknown subcommand "lsit" for "agentsfleet"\n' +
      "  code          UNKNOWN_COMMAND\n" +
      "  suggestion    run `agentsfleet --help` for the command list\n",
    );
  });

  test("a guard refusal moves its fix onto the suggestion line", async () => {
    const result = await run(FAILURES[2]!, { json: false, columns: 120 });
    expect(result.err).toContain("✕ error: not authenticated\n");
    expect(result.err).toMatch(/^ {2}suggestion {4}run `agentsfleet login` first$/m);
  });

  test("the reported value column is the pinned one", () => {
    expect(BLOCK_VALUE_COLUMN).toBe(VALUE_INDEX + 1);
  });

  test("off a terminal a long value stays on one line", () => {
    const value = "word ".repeat(40).trim();
    const text = renderErrorBlock({ severity: "error", message: "m", fields: { suggestion: value } }, Number.POSITIVE_INFINITY, PLAIN_STYLE);
    expect(text.split("\n")).toHaveLength(2);
    expect(splitFix("no fix here")).toEqual({ message: "no fix here", fix: undefined });
  });
});

// Recorded from each failure at the commit before the block existed; the
// `request_id` is the one this stub's 403 body carries.
const BASE_ENVELOPES: Readonly<Record<string, unknown>> = {
  "server error": {
    error: {
      code: "UZ-AUTH-022",
      message: "Missing fleet read scope",
      suggestion: "ask your administrator to grant the required permissions",
      request_id: "req_block_1",
    },
  },
  "network error": {
    error: {
      code: "NetworkError",
      message: `cannot reach agentsfleet API at ${API_URL}/v1/workspaces/${WORKSPACE_ID}/fleets/${FLEET_ID}`,
      suggestion: "check network connectivity, AGENTSFLEET_API_URL, and any proxy/VPN settings",
    },
  },
  "guard refusal": { error: { code: "AUTH_REQUIRED", message: "not authenticated — run `agentsfleet login` first" } },
  "usage error": {
    error: {
      code: "INVALID_ARGUMENT",
      message: `invalid fleet_id: expected uuidv7 format (e.g. ${FLEET_ID}) (got "abc")`,
    },
  },
  "unknown command": {
    error: { code: "UNKNOWN_COMMAND", message: 'Unknown subcommand "lsit" for "agentsfleet workspace"\n\n  Did you mean this?\n    list' },
  },
  // New with this change: the flag did not exist before.
  "invalid completions shell": {
    error: {
      code: "INVALID_ARGUMENT",
      message: 'invalid --completions: "bash" | "zsh" | "fish" | "powershell" | "sh" | "pwsh" (got "tcsh")',
    },
  },
};

describe("json_error_envelope_is_unchanged", () => {
  for (const failure of FAILURES) {
    test(`${failure.name} keeps its envelope`, async () => {
      const result = await run(failure, { json: true, columns: 120 });
      expect(result.out).toBe("");
      expect(JSON.parse(result.err)).toEqual(BASE_ENVELOPES[failure.name]);
    });
  }
});

describe("requested_help_prints_unchanged_document", () => {
  test.each([[["fleet", "--help"]], [["fleet"]]])("%p prints help on stdout and no block", async (argv) => {
    const result = await run({ name: "help", argv, signedIn: false, reply: "forbidden" }, { json: false, columns: 120 });
    expect(result.code).toBe(0);
    expect(result.out).toContain("DESCRIPTION");
    expect(result.out).toContain("SUBCOMMANDS");
    expect(result.err).toBe("");
  });
});
