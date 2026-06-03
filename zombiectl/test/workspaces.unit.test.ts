// Direct unit tests for src/services/workspaces.ts.
//
// Three surfaces under test:
//   1. workspacesLayer  — wraps the on-disk loadWorkspacesRaw / saveWorkspacesRaw
//      via Effect.tryPromise; error path (lines 30-35) fires when the
//      underlying fs call throws (unreadable file).
//   2. workspacesFromValueLayer — in-memory snapshot variant (lines 52-64).
//      load returns current, save replaces it; isolated from disk entirely.
//
// Every test runs inside withFreshStateDir so disk reads/writes go to a
// temp dir; process.env.ZOMBIE_STATE_DIR is restored on exit.

import { describe, test, expect } from "bun:test";
import fs from "node:fs/promises";
import path from "node:path";
import { Cause, Effect, Exit, Option } from "effect";
import {
  Workspaces,
  workspacesLayer,
  workspacesFromValueLayer,
  type WorkspacesValue,
} from "../src/services/workspaces.ts";
import { UnexpectedError } from "../src/errors/index.ts";
import { withFreshStateDir } from "./helpers-cli-state.ts";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const EMPTY_WS: WorkspacesValue = { current_workspace_id: null, items: [] };

const WS_A: WorkspacesValue = {
  current_workspace_id: "ws-1",
  items: [{ workspace_id: "ws-1", name: "Alpha", created_at: 1_000_000 }],
};

const WS_B: WorkspacesValue = {
  current_workspace_id: "ws-2",
  items: [
    { workspace_id: "ws-1", name: "Alpha", created_at: 1_000_000 },
    { workspace_id: "ws-2", name: "Beta", created_at: 2_000_000 },
  ],
};

/** Run an Effect against workspacesLayer and extract the success value. */
async function runLoad(stateDir: string): Promise<WorkspacesValue> {
  void stateDir; // ZOMBIE_STATE_DIR already set by withFreshStateDir
  return Effect.runPromise(
    Effect.flatMap(Workspaces, (svc) => svc.load).pipe(
      Effect.provide(workspacesLayer),
    ),
  );
}

async function runSave(
  stateDir: string,
  next: WorkspacesValue,
): Promise<void> {
  void stateDir;
  return Effect.runPromise(
    Effect.flatMap(Workspaces, (svc) => svc.save(next)).pipe(
      Effect.provide(workspacesLayer),
    ),
  );
}

// ---------------------------------------------------------------------------
// workspacesLayer — disk-backed
// ---------------------------------------------------------------------------

describe("workspacesLayer — load (disk)", () => {
  test("returns empty defaults when workspaces.json does not exist", async () => {
    const result = await withFreshStateDir(async (dir) => runLoad(dir));
    expect(result.current_workspace_id).toBeNull();
    expect(result.items).toHaveLength(0);
  });

  test("round-trip: save then load returns the saved value", async () => {
    const result = await withFreshStateDir(async (dir) => {
      await runSave(dir, WS_A);
      return runLoad(dir);
    });
    expect(result.current_workspace_id).toBe("ws-1");
    expect(result.items).toHaveLength(1);
    expect(result.items[0]?.name).toBe("Alpha");
  });

  test("second save overwrites the first", async () => {
    const result = await withFreshStateDir(async (dir) => {
      await runSave(dir, WS_A);
      await runSave(dir, WS_B);
      return runLoad(dir);
    });
    expect(result.current_workspace_id).toBe("ws-2");
    expect(result.items).toHaveLength(2);
  });

  test("null workspace_id and null-name item persists correctly", async () => {
    const withNullName: WorkspacesValue = {
      current_workspace_id: null,
      items: [{ workspace_id: "ws-null-name", name: null, created_at: null }],
    };
    const result = await withFreshStateDir(async (dir) => {
      await runSave(dir, withNullName);
      return runLoad(dir);
    });
    expect(result.current_workspace_id).toBeNull();
    expect(result.items[0]?.name).toBeNull();
    expect(result.items[0]?.created_at).toBeNull();
  });
});

describe("workspacesLayer — load error path", () => {
  test("non-ENOENT IO error surfaces as UnexpectedError with the cause embedded", async () => {
    // A directory where the state file is expected makes readFile throw
    // EISDIR — a non-ENOENT, non-SyntaxError error that propagates through
    // readJson to the load catch. Deterministic for every uid, unlike chmod
    // 000 (which the file owner / root can still read).
    const exit = await withFreshStateDir(async (dir) => {
      await fs.mkdir(path.join(dir, "workspaces.json"));
      return Effect.runPromiseExit(
        Effect.flatMap(Workspaces, (svc) => svc.load).pipe(
          Effect.provide(workspacesLayer),
        ),
      );
    });

    expect(Exit.isFailure(exit)).toBe(true);
    const err = Exit.isFailure(exit)
      ? Option.getOrNull(Cause.findErrorOption(exit.cause))
      : null;
    expect(err).toBeInstanceOf(UnexpectedError);
    const ue = err as UnexpectedError;
    expect(ue.detail).toMatch(/workspaces load failed/);
    // The catch embeds the underlying Error.message (the EISDIR text).
    expect(ue.detail).toMatch(/EISDIR|illegal operation on a directory/i);
    expect(ue.suggestion).toMatch(/permissions/);
  });
});

describe("workspacesLayer — save error path (lines 30-35)", () => {
  test("UnexpectedError when state dir is a file (save cannot mkdir)", async () => {
    const exit = await withFreshStateDir(async (dir) => {
      // Replace the dir with a plain file so mkdir inside writeJson throws.
      const blocker = path.join(dir, "blocker");
      await fs.writeFile(blocker, "x");
      const prevDir = process.env.ZOMBIE_STATE_DIR;
      process.env.ZOMBIE_STATE_DIR = blocker;
      try {
        return await Effect.runPromiseExit(
          Effect.flatMap(Workspaces, (svc) => svc.save(EMPTY_WS)).pipe(
            Effect.provide(workspacesLayer),
          ),
        );
      } finally {
        if (prevDir === undefined) delete process.env.ZOMBIE_STATE_DIR;
        else process.env.ZOMBIE_STATE_DIR = prevDir;
      }
    });

    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const err = Option.getOrNull(Cause.findErrorOption(exit.cause));
      expect(err).toBeInstanceOf(UnexpectedError);
      const ue = err as UnexpectedError;
      expect(ue.detail).toMatch(/workspaces save failed/);
      expect(ue.suggestion).toMatch(/permissions/);
    }
  });
});

// ---------------------------------------------------------------------------
// workspacesFromValueLayer — in-memory snapshot (lines 52-64)
// ---------------------------------------------------------------------------

describe("workspacesFromValueLayer — in-memory", () => {
  test("load returns the initial value", async () => {
    const layer = workspacesFromValueLayer(WS_A);
    const result = await Effect.runPromise(
      Effect.flatMap(Workspaces, (svc) => svc.load).pipe(
        Effect.provide(layer),
      ),
    );
    expect(result.current_workspace_id).toBe("ws-1");
    expect(result.items).toHaveLength(1);
  });

  test("save then load reflects the saved value", async () => {
    const layer = workspacesFromValueLayer(EMPTY_WS);
    const result = await Effect.runPromise(
      Effect.flatMap(Workspaces, (svc) =>
        Effect.flatMap(svc.save(WS_A), () => svc.load),
      ).pipe(Effect.provide(layer)),
    );
    expect(result.current_workspace_id).toBe("ws-1");
    expect(result.items[0]?.name).toBe("Alpha");
  });

  test("multiple saves accumulate correctly — last write wins", async () => {
    const layer = workspacesFromValueLayer(EMPTY_WS);
    const result = await Effect.runPromise(
      Effect.flatMap(Workspaces, (svc) =>
        Effect.flatMap(svc.save(WS_A), () =>
          Effect.flatMap(svc.save(WS_B), () => svc.load),
        ),
      ).pipe(Effect.provide(layer)),
    );
    expect(result.current_workspace_id).toBe("ws-2");
    expect(result.items).toHaveLength(2);
  });

  test("initial value is defensively cloned — external mutation does not bleed in", async () => {
    const mutable: WorkspacesValue = {
      current_workspace_id: "ws-orig",
      items: [{ workspace_id: "ws-orig", name: "Orig", created_at: 0 }],
    };
    const layer = workspacesFromValueLayer(mutable);
    // Mutate the source after layer construction.
    (mutable as { current_workspace_id: string | null }).current_workspace_id = "mutated";
    (mutable.items as WorkspacesValue["items"]).push({
      workspace_id: "extra",
      name: "Extra",
      created_at: 1,
    });
    const result = await Effect.runPromise(
      Effect.flatMap(Workspaces, (svc) => svc.load).pipe(
        Effect.provide(layer),
      ),
    );
    expect(result.current_workspace_id).toBe("ws-orig");
    expect(result.items).toHaveLength(1);
  });

  test("saved value is defensively cloned — post-save mutation does not change state", async () => {
    const layer = workspacesFromValueLayer(EMPTY_WS);
    const saved: WorkspacesValue = {
      current_workspace_id: "ws-snap",
      items: [{ workspace_id: "ws-snap", name: "Snap", created_at: 0 }],
    };
    await Effect.runPromise(
      Effect.flatMap(Workspaces, (svc) => svc.save(saved)).pipe(
        Effect.provide(layer),
      ),
    );
    // Mutate the value that was passed to save.
    (saved as { current_workspace_id: string | null }).current_workspace_id = "corrupted";
    const result = await Effect.runPromise(
      Effect.flatMap(Workspaces, (svc) => svc.load).pipe(
        Effect.provide(layer),
      ),
    );
    expect(result.current_workspace_id).toBe("ws-snap");
  });

  test("empty items list is preserved across save/load", async () => {
    const layer = workspacesFromValueLayer(WS_A);
    const cleared: WorkspacesValue = { current_workspace_id: null, items: [] };
    const result = await Effect.runPromise(
      Effect.flatMap(Workspaces, (svc) =>
        Effect.flatMap(svc.save(cleared), () => svc.load),
      ).pipe(Effect.provide(layer)),
    );
    expect(result.current_workspace_id).toBeNull();
    expect(result.items).toHaveLength(0);
  });
});
