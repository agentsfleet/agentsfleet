// Function-coverage fillers for three effect-services whose LINES were
// already 100% but whose FUNCTIONS lagged: the free helpers on
// CommandRuntime, the live readline `defaultCreateInterface` factory in
// the Input service, and the env-resolver + both Layer constructors on
// CliConfig. Each declared free function / method closure is invoked
// once so bun's per-function metric credits it.
//
// Note on the residual per-file ceiling: each of these files carries one
// `Context.Service` tag modules (CommandRuntime / Input / CliConfig). Bun
// counts the synthesized service constructor as a function it cannot
// exercise — Layer.succeed/Layer.effect register the shape, they never
// `new` the class, and `new Service(shape)` does not credit it either
// (verified empirically against bun 1.3.x). That single synthesized fn
// per file is the documented slack the bunfig 96% function floor
// accommodates (see bunfig.toml header). These tests close every
// *real* function on each file.

import { describe, test, expect } from "bun:test";
import { Effect } from "effect";
import {
  CommandRuntime,
  commandRuntimeFromValuesLayer,
  getCommandRuntimeCommand,
  getCommandRuntimeSpanName,
} from "../src/runtime/command-runtime.service.ts";
import { Input, inputLayer, makeLive } from "../src/services/input.ts";
import {
  CliConfig,
  cliConfigLayer,
  cliConfigFromValuesLayer,
  resolveCliConfig,
} from "../src/services/config.ts";

// config.ts keeps DEFAULT_API_URL unexported (internal default); pin the
// fallback literal here so a drift in the resolver's default is caught.
const DEFAULT_API_URL = "https://api.agentsfleet.net";

// ── CommandRuntime free helpers ──────────────────────────────────────────
// getCommandRuntimeCommand / getCommandRuntimeSpanName are only reached
// transitively (command-instrumentation) elsewhere; pin their exact
// join/format here so a drift in the separator is caught directly.

describe("CommandRuntime helpers", () => {
  test("getCommandRuntimeCommand joins commandPath with spaces", () => {
    expect(
      getCommandRuntimeCommand({
        commandPath: ["workspace", "add"],
        commandRunId: "rid-1",
      }),
    ).toBe("workspace add");
  });

  test("getCommandRuntimeCommand on a single-segment path returns that segment", () => {
    expect(
      getCommandRuntimeCommand({ commandPath: ["login"], commandRunId: "r" }),
    ).toBe("login");
  });

  test("getCommandRuntimeCommand on an empty path returns the empty string", () => {
    expect(
      getCommandRuntimeCommand({ commandPath: [], commandRunId: "r" }),
    ).toBe("");
  });

  test("getCommandRuntimeSpanName dot-joins under the cli. prefix", () => {
    expect(
      getCommandRuntimeSpanName({
        commandPath: ["workspace", "add"],
        commandRunId: "rid-2",
      }),
    ).toBe("cli.workspace.add");
  });

  test("getCommandRuntimeSpanName on an empty path is the bare cli. prefix", () => {
    expect(
      getCommandRuntimeSpanName({ commandPath: [], commandRunId: "r" }),
    ).toBe("cli.");
  });

  test("commandRuntimeFromValuesLayer resolves a CommandRuntime carrying the supplied values", async () => {
    const layer = commandRuntimeFromValuesLayer({
      commandPath: ["runner", "register"],
      commandRunId: "rid-3",
    });
    const out = await Effect.runPromise(
      Effect.gen(function* () {
        return yield* CommandRuntime;
      }).pipe(Effect.provide(layer)),
    );
    expect(out.commandRunId).toBe("rid-3");
    expect(getCommandRuntimeCommand(out)).toBe("runner register");
    expect(getCommandRuntimeSpanName(out)).toBe("cli.runner.register");
  });
});

// ── Input live factory ───────────────────────────────────────────────────
// makeLive()'s default param is `defaultCreateInterface`, which builds a
// real node:readline interface over process.stdin/out. Existing tests
// inject a fake factory, so the default arrow never ran. We exercise it
// with a pre-aborted AbortSignal: the real readline `question` rejects
// immediately (AbortError) → the catch maps to null → finally closes the
// interface. No real keystroke is awaited, so the runner never blocks.

describe("Input.makeLive default readline factory", () => {
  test("default createInterface + pre-aborted signal resolves to null", async () => {
    const ctrl = new AbortController();
    ctrl.abort();
    const live = makeLive(); // default factory → defaultCreateInterface
    const result = await Effect.runPromise(live.readLine("Enter code: ", ctrl.signal));
    expect(result).toBeNull();
  });

  test("inputLayer's live service is callable and honours the abort path", async () => {
    const ctrl = new AbortController();
    ctrl.abort();
    const program = Effect.gen(function* () {
      const input = yield* Input;
      return yield* input.readLine("code: ", ctrl.signal);
    });
    const result = await Effect.runPromise(program.pipe(Effect.provide(inputLayer)));
    expect(result).toBeNull();
  });
});

// ── CliConfig resolver + layer constructors ──────────────────────────────
// resolveCliConfig drives the readEnv + trimmed helpers; both the
// effect-backed cliConfigLayer and the override-merging
// cliConfigFromValuesLayer are realised through the service so their
// closures run.

describe("CliConfig resolver and layers", () => {
  test("resolveCliConfig reads env (readEnv + trimmed) into apiUrl", () => {
    const prev = process.env.AGENTSFLEET_API_URL;
    process.env.AGENTSFLEET_API_URL = "  https://probe.api.local  ";
    try {
      const cfg = resolveCliConfig();
      // trimmed() strips the padding; readEnv() sourced it.
      expect(cfg.apiUrl).toBe("https://probe.api.local");
    } finally {
      if (prev === undefined) delete process.env.AGENTSFLEET_API_URL;
      else process.env.AGENTSFLEET_API_URL = prev;
    }
  });

  test("resolveCliConfig falls back to the default api url when env is unset", () => {
    const prev = process.env.AGENTSFLEET_API_URL;
    delete process.env.AGENTSFLEET_API_URL;
    try {
      expect(resolveCliConfig().apiUrl).toBe(DEFAULT_API_URL);
    } finally {
      if (prev !== undefined) process.env.AGENTSFLEET_API_URL = prev;
    }
  });

  test("cliConfigLayer (Layer.effect) realises a CliConfig with a string apiUrl", async () => {
    const cfg = await Effect.runPromise(
      Effect.gen(function* () {
        return yield* CliConfig;
      }).pipe(Effect.provide(cliConfigLayer)),
    );
    expect(typeof cfg.apiUrl).toBe("string");
    expect(cfg.jsonMode).toBe(false);
  });

  test("cliConfigFromValuesLayer merges overrides over the env-resolved base", async () => {
    const cfg = await Effect.runPromise(
      Effect.gen(function* () {
        return yield* CliConfig;
      }).pipe(
        Effect.provide(
          cliConfigFromValuesLayer({ jsonMode: true, noOpen: true }),
        ),
      ),
    );
    expect(cfg.jsonMode).toBe(true);
    expect(cfg.noOpen).toBe(true);
  });
});
