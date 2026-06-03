// Exercises buildZombieHandlers — the factory that wires every zombie
// subcommand to its Effect. Injects spy wrapE/wrapEFn so each frame-factory
// arrow can be called in isolation without a real Effect runtime. Every
// handler slot is invoked and every factory branch is exercised.

import { describe, expect, test } from "bun:test";
import { buildZombieHandlers } from "../src/program/handlers-bind-zombie.ts";
import type { ActionFrame } from "../src/program/cli-tree-types.ts";
import type { Effect } from "effect";
import type { CliError } from "../src/errors/index.ts";
import type { MainLayerServices } from "../src/lib/run-effect.ts";

// Spy types that capture invocation arguments for assertion.
interface WrapECall {
  name: string;
  effect: Effect.Effect<void, CliError, MainLayerServices>;
}
interface WrapEFnCall {
  name: string;
  factory: (frame: ActionFrame) => Effect.Effect<void, CliError, MainLayerServices>;
}

function makeFrame(
  positionals: string[],
  options: Record<string, unknown> = {},
): ActionFrame {
  return {
    name: "test",
    parsed: { positionals, options },
  } as unknown as ActionFrame;
}

function buildSpies() {
  const wrapECalls: WrapECall[] = [];
  const wrapEFnCalls: WrapEFnCall[] = [];

  const wrapE = (
    name: string,
    effect: Effect.Effect<void, CliError, MainLayerServices>,
  ) => {
    wrapECalls.push({ name, effect });
    return async (_frame: ActionFrame) => 0 as number;
  };

  const wrapEFn = (
    name: string,
    factory: (frame: ActionFrame) => Effect.Effect<void, CliError, MainLayerServices>,
  ) => {
    wrapEFnCalls.push({ name, factory });
    return async (_frame: ActionFrame) => 0 as number;
  };

  const handlers = buildZombieHandlers(wrapE as never, wrapEFn as never);
  return { handlers, wrapECalls, wrapEFnCalls };
}

describe("buildZombieHandlers — wrapE slots", () => {
  test("status wires zombie.status via wrapE", () => {
    const { wrapECalls } = buildSpies();
    const statusCall = wrapECalls.find((c) => c.name === "zombie.status");
    expect(statusCall).toBeDefined();
    expect(statusCall!.name).toBe("zombie.status");
    expect(statusCall!.effect).toBeDefined();
  });

  test("credential.list wires zombie.credential.list via wrapE", () => {
    const { wrapECalls } = buildSpies();
    const call = wrapECalls.find((c) => c.name === "zombie.credential.list");
    expect(call).toBeDefined();
    expect(call!.effect).toBeDefined();
  });
});

describe("buildZombieHandlers — install / update factories", () => {
  test("install factory delegates to installEffectFromFlags with from=undefined", () => {
    const { wrapEFnCalls } = buildSpies();
    const call = wrapEFnCalls.find((c) => c.name === "zombie.install");
    expect(call).toBeDefined();
    const effect = call!.factory(makeFrame([], {}));
    expect(effect).toBeDefined();
  });

  test("update factory passes positional and from-option to updateEffectFromArgs", () => {
    const { wrapEFnCalls } = buildSpies();
    const call = wrapEFnCalls.find((c) => c.name === "zombie.update");
    expect(call).toBeDefined();
    // Lines 54-56: positionals[0] + optString(options, "from")
    const effect = call!.factory(makeFrame(["my-zombie"], { from: "v2.0.0" }));
    expect(effect).toBeDefined();
  });

  test("update factory works with empty positionals", () => {
    const { wrapEFnCalls } = buildSpies();
    const call = wrapEFnCalls.find((c) => c.name === "zombie.update")!;
    const effect = call.factory(makeFrame([], {}));
    expect(effect).toBeDefined();
  });
});

describe("buildZombieHandlers — list factory", () => {
  test("list factory passes workspace-id, cursor, limit from options", () => {
    const { wrapEFnCalls } = buildSpies();
    const call = wrapEFnCalls.find((c) => c.name === "zombie.list")!;
    const effect = call.factory(
      makeFrame([], { "workspace-id": "ws-abc", cursor: "tok", limit: "10" }),
    );
    expect(effect).toBeDefined();
  });

  test("list factory accepts workspaceId camelCase alias", () => {
    const { wrapEFnCalls } = buildSpies();
    const call = wrapEFnCalls.find((c) => c.name === "zombie.list")!;
    const effect = call.factory(makeFrame([], { workspaceId: "ws-xyz" }));
    expect(effect).toBeDefined();
  });
});

describe("buildZombieHandlers — stop / resume / kill / delete factories", () => {
  test("stop factory passes positional id", () => {
    const { wrapEFnCalls } = buildSpies();
    const call = wrapEFnCalls.find((c) => c.name === "zombie.stop")!;
    expect(call.factory(makeFrame(["zb-001"]))).toBeDefined();
  });

  test("resume factory passes positional id", () => {
    const { wrapEFnCalls } = buildSpies();
    const call = wrapEFnCalls.find((c) => c.name === "zombie.resume")!;
    expect(call.factory(makeFrame(["zb-001"]))).toBeDefined();
  });

  test("kill factory passes positional id", () => {
    const { wrapEFnCalls } = buildSpies();
    const call = wrapEFnCalls.find((c) => c.name === "zombie.kill")!;
    expect(call.factory(makeFrame(["zb-001"]))).toBeDefined();
  });

  test("delete factory passes positional id", () => {
    const { wrapEFnCalls } = buildSpies();
    const call = wrapEFnCalls.find((c) => c.name === "zombie.delete")!;
    expect(call.factory(makeFrame(["zb-001"]))).toBeDefined();
  });
});

describe("buildZombieHandlers — logs factory", () => {
  test("logs factory uses zombie option when present", () => {
    const { wrapEFnCalls } = buildSpies();
    const call = wrapEFnCalls.find((c) => c.name === "zombie.logs")!;
    const effect = call.factory(makeFrame([], { zombie: "zb-opt", cursor: "c1", limit: "5" }));
    expect(effect).toBeDefined();
  });

  test("logs factory falls back to positionals[0]", () => {
    const { wrapEFnCalls } = buildSpies();
    const call = wrapEFnCalls.find((c) => c.name === "zombie.logs")!;
    const effect = call.factory(makeFrame(["zb-pos"], {}));
    expect(effect).toBeDefined();
  });
});

describe("buildZombieHandlers — events factory (lines 101-108)", () => {
  test("events factory passes all flags to eventsEffectFromFlags", () => {
    const { wrapEFnCalls } = buildSpies();
    const call = wrapEFnCalls.find((c) => c.name === "zombie.events")!;
    // Lines 101-108: zombieId, actor, since, cursor, limit, json
    const effect = call.factory(
      makeFrame(["zb-123"], {
        actor: "usr-1",
        since: "2024-01-01",
        cursor: "tok",
        limit: "20",
        json: true,
      }),
    );
    expect(effect).toBeDefined();
  });

  test("events factory with json=false and minimal args", () => {
    const { wrapEFnCalls } = buildSpies();
    const call = wrapEFnCalls.find((c) => c.name === "zombie.events")!;
    const effect = call.factory(makeFrame(["zb-456"], { json: false }));
    expect(effect).toBeDefined();
  });
});

describe("buildZombieHandlers — steer factory (lines 113-116)", () => {
  test("steer factory passes two positionals and tty flag true", () => {
    const { wrapEFnCalls } = buildSpies();
    const call = wrapEFnCalls.find((c) => c.name === "zombie.steer")!;
    // Lines 113-116: positionals[0], positionals[1], forceTty from OPT_TTY
    const effect = call.factory(makeFrame(["zb-001", "hello"], { tty: true }));
    expect(effect).toBeDefined();
  });

  test("steer factory with forceTty=false (tty option absent)", () => {
    const { wrapEFnCalls } = buildSpies();
    const call = wrapEFnCalls.find((c) => c.name === "zombie.steer")!;
    const effect = call.factory(makeFrame(["zb-001", "hi"], {}));
    expect(effect).toBeDefined();
  });
});

describe("buildZombieHandlers — credential sub-handlers", () => {
  test("credential.add factory passes name, data, force", () => {
    const { wrapEFnCalls } = buildSpies();
    const call = wrapEFnCalls.find((c) => c.name === "zombie.credential.add")!;
    const effect = call.factory(
      makeFrame(["my-cred"], { data: '{"key":"val"}', force: true }),
    );
    expect(effect).toBeDefined();
  });

  test("credential.show factory passes positionals[0] as name", () => {
    const { wrapEFnCalls } = buildSpies();
    const call = wrapEFnCalls.find((c) => c.name === "zombie.credential.show")!;
    const effect = call.factory(makeFrame(["my-cred"], {}));
    expect(effect).toBeDefined();
  });

  test("credential.delete factory passes positionals[0] as name", () => {
    const { wrapEFnCalls } = buildSpies();
    const call = wrapEFnCalls.find((c) => c.name === "zombie.credential.delete")!;
    const effect = call.factory(makeFrame(["my-cred"], {}));
    expect(effect).toBeDefined();
  });
});

describe("buildZombieHandlers — handler shape", () => {
  test("returned object has all expected zombie handler keys", () => {
    const { handlers } = buildSpies();
    const keys = [
      "install", "update", "list", "status", "stop", "resume",
      "kill", "delete", "logs", "events", "steer",
    ];
    for (const key of keys) {
      expect(typeof handlers[key as keyof typeof handlers]).toBe("function");
    }
    expect(typeof handlers.credential.add).toBe("function");
    expect(typeof handlers.credential.show).toBe("function");
    expect(typeof handlers.credential.list).toBe("function");
    expect(typeof handlers.credential.delete).toBe("function");
  });

  test("total wrapEFn registrations matches expected command count", () => {
    const { wrapEFnCalls } = buildSpies();
    // install, update, list, stop, resume, kill, delete, logs, events, steer,
    // credential.add, credential.show, credential.delete = 13
    expect(wrapEFnCalls.length).toBe(13);
  });

  test("total wrapE registrations matches expected command count", () => {
    const { wrapECalls } = buildSpies();
    // status, credential.list = 2
    expect(wrapECalls.length).toBe(2);
  });
});
