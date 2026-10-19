// `agentsfleet models` — the CLI peer of the dashboard's model picker.
//
// Before this command the CLI had no way to ask what a provider or model id
// should be: `--provider` was checked against a vendored copy of the retired
// runner's dial tables and `--model` was checked against nothing, so the flow was
// "type two identifiers blind, find out at run time". These cases pin the
// discovery surface that replaced it, against the real HTTP layer.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir, cliEnv } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

import { catalogueModel, cataloguePage as page } from "./helpers-catalogue.ts";

const WS_ID = "ws_models_test";
const MILLION_CONTEXT_TOKENS = 1_000_000;

const authedScope = <T>(fn: () => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_models" }, fn);

const PRICED = catalogueModel("claude-opus-5", "anthropic");
const UNPRICED = catalogueModel("qwen3.7-max", "qwen", {
  context_cap_tokens: MILLION_CONTEXT_TOKENS, input_nanos_per_mtok: "0", cached_input_nanos_per_mtok: "0",
  output_nanos_per_mtok: "0",
});

describe("agentsfleet models", () => {
  test("renders every priced row with its provider, context window, and rates", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        "GET /v1/models": () => jsonResponse(200, page([PRICED, UNPRICED])),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["models"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl, NO_COLOR: "1" }),
        });
        expect(code).toBe(0);
        const text = out.read() + err.read();
        expect(text).toContain("anthropic");
        expect(text).toContain("claude-opus-5");
        // The context limit stays exact in both display modes.
        expect(text).toContain("200,000");
        // nanos per Mtok → dollars per Mtok.
        expect(text).toContain("$5.00");
        expect(text).toContain("$25.00");
        expect(text).toContain("$0.00");
        expect(text).toContain("CACHED/MTOK");
      });
    });
  });

  test("a sub-cent rate keeps its digits instead of collapsing to $0.00", async () => {
    await authedScope(async () => {
      // A sub-cent quote retains all six fractional digits.
      const CHEAP = catalogueModel("deepseek-flash", "deepseek", {
        context_cap_tokens: MILLION_CONTEXT_TOKENS, input_nanos_per_mtok: "3625000",
        cached_input_nanos_per_mtok: "3625000", output_nanos_per_mtok: "280000000",
      });
      const routes: MockRoutes = {
        "GET /v1/models": () => jsonResponse(200, page([CHEAP])),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["models"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl, NO_COLOR: "1" }),
        });
        expect(code).toBe(0);
        const text = out.read() + err.read();
        expect(text).toContain("$0.003625");
        expect(text).not.toContain("$0.00 ");
        // …and it is still distinguishable from a genuinely unpriced row.
        expect(text).not.toContain("—");
      });
    });
  });

  test("--provider filters server-side and reports the count it found", async () => {
    await authedScope(async () => {
      const seen: string[] = [];
      const routes: MockRoutes = {
        "GET /v1/models": (_req, url) => {
          seen.push(url.search);
          return jsonResponse(200, page([PRICED]));
        },
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["models", "--provider", "anthropic"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl, NO_COLOR: "1" }),
        });
        expect(code).toBe(0);
        // The endpoint owns the filter; the CLI must not fetch everything and
        // narrow locally, which would page the whole catalogue to show one row.
        expect(seen[0]).toContain("provider=anthropic");
        const text = out.read() + err.read();
        expect(text).toContain("1 model(s)");
      });
    });
  });

  test("follows next_cursor so a multi-page catalogue is not silently truncated", async () => {
    await authedScope(async () => {
      let call = 0;
      const routes: MockRoutes = {
        "GET /v1/models": () => {
          call += 1;
          return call === 1
            ? jsonResponse(200, page([PRICED], "cur-1"))
            : jsonResponse(200, page([UNPRICED]));
        },
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["models"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl, NO_COLOR: "1" }),
        });
        expect(code).toBe(0);
        expect(call).toBe(2);
        const text = out.read() + err.read();
        expect(text).toContain("claude-opus-5");
        expect(text).toContain("qwen3.7-max");
        expect(text).toContain("2 model(s) across 2 provider(s)");
      });
    });
  });

  test("--json emits the raw catalogue rows, not the rendered table", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        "GET /v1/models": () => jsonResponse(200, page([PRICED])),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["models", "--json"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        err.read();
        const parsed = JSON.parse(out.read()) as { models: Array<{ provider: string; input_nanos_per_mtok: string }> };
        // Nanos, unrounded: a script consuming this must not inherit the
        // two-decimal rounding the table applies for humans.
        expect(parsed.models[0]?.provider).toBe("anthropic");
        expect(parsed.models[0]?.input_nanos_per_mtok).toBe("5000000000");
      });
    });
  });

  test("an empty catalogue explains that an admin primes it, and still exits 0", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        "GET /v1/models": () => jsonResponse(200, page([])),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["models"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl, NO_COLOR: "1" }),
        });
        // A catalogue nobody has primed yet is a provisioning state, not a
        // failure — exiting non-zero would break a provisioning script here.
        expect(code).toBe(0);
        const text = out.read() + err.read();
        expect(text).toContain("empty");
        expect(text).toContain("admin");
      });
    });
  });

  test("an empty PROVIDER-scoped result names the provider and points at the full list", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        "GET /v1/models": () => jsonResponse(200, page([])),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["models", "--provider", "cerebras"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl, NO_COLOR: "1" }),
        });
        expect(code).toBe(0);
        const text = out.read() + err.read();
        expect(text).toContain("cerebras");
        expect(text).toContain("agentsfleet models");
      });
    });
  });

  test("a catalogue read failure surfaces as an error, not an empty table", async () => {
    await authedScope(async () => {
      // `models` cannot degrade the way `--provider` does: its entire output IS
      // the catalogue, so printing "no models" on a 503 would report an outage
      // as an empty product.
      const routes: MockRoutes = {
        "GET /v1/models": () => jsonResponse(503, { detail: "catalogue down" }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["models"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl, NO_COLOR: "1" }),
        });
        expect(code).not.toBe(0);
        const text = out.read() + err.read();
        expect(text).not.toContain("catalogue is empty");
      });
    });
  });
});
