import { readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
// The bundle guard below AST-walks production sources, which needs the
// JavaScript compiler API that the Go-native typescript@7 no longer ships —
// `typescript-jsapi` aliases typescript@6 purely as this test's parser.
import ts from "typescript-jsapi";
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";
import {
  createIntentModuleLoader,
  INTENT_MODULE_STATUS,
  maySpeculateOnHover,
  useIntentModule,
} from "./intent-module-loader";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((next, fail) => {
    resolve = next;
    reject = fail;
  });
  return { promise, reject, resolve };
}

function stubPointer(coarse: boolean) {
  vi.stubGlobal(
    "matchMedia",
    vi.fn((query: string) => ({
      matches: coarse && query.includes("coarse"),
      media: query,
    })),
  );
}

function readAppSource(path: string): string {
  return readFileSync(resolve(process.cwd(), path), "utf8");
}

function productionSourceFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) return productionSourceFiles(path);
    if (
      !entry.name.match(/\.tsx?$/) ||
      entry.name.includes(".test.") ||
      entry.name.includes(".spec.")
    ) {
      return [];
    }
    return [path];
  });
}

function staticModuleSpecifiers(path: string): string[] {
  const source = readFileSync(path, "utf8");
  const kind = path.endsWith(".tsx")
    ? ts.ScriptKind.TSX
    : ts.ScriptKind.TS;
  const parsed = ts.createSourceFile(
    path,
    source,
    ts.ScriptTarget.Latest,
    true,
    kind,
  );
  const specifiers: string[] = [];

  function visit(node: ts.Node) {
    if (
      (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) &&
      node.moduleSpecifier &&
      ts.isStringLiteral(node.moduleSpecifier)
    ) {
      specifiers.push(node.moduleSpecifier.text);
    }
    const [requiredModule] = ts.isCallExpression(node)
      ? node.arguments
      : [];
    if (
      ts.isCallExpression(node) &&
      ts.isIdentifier(node.expression) &&
      node.expression.text === "require" &&
      node.arguments.length === 1 &&
      requiredModule &&
      ts.isStringLiteral(requiredModule)
    ) {
      specifiers.push(requiredModule.text);
    }
    ts.forEachChild(node, visit);
  }

  visit(parsed);
  return specifiers;
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("intent module loader", () => {
  it("test_closed_heavy_tools_stay_out_of_initial_entries", async () => {
    const initialEntries = [
      [
        "app/(dashboard)/admin/models/components/ModelsView.tsx",
        'from "./AddModelDialog"',
        "AddModelDialogDynamic",
      ],
      [
        "app/(dashboard)/admin/models/components/CatalogueList.tsx",
        'from "./EditModelDialog"',
        "EditModelDialogDynamic",
      ],
      [
        "app/(dashboard)/admin/fleet-libraries/components/FleetLibrariesView.tsx",
        'from "./AddFleetDialog"',
        "AddFleetDialogDynamic",
      ],
      [
        "app/(dashboard)/admin/fleet-libraries/components/PlatformCatalogTable.tsx",
        'from "./EditFleetDialog"',
        "EditFleetDialogDynamic",
      ],
      [
        "app/(dashboard)/w/[workspaceId]/fleets/new/InstallSourceSelector.tsx",
        'from "./AddLibraryDialog"',
        "AddLibraryDialogDynamic",
      ],
    ] as const;

    for (const [path, eagerImport, dynamicBoundary] of initialEntries) {
      const source = readAppSource(path);
      expect(source).not.toContain(eagerImport);
      expect(source).toContain(dynamicBoundary);
    }

    const heavyModules = new Set([
      "AddFleetDialog",
      "AddLibraryDialog",
      "AddModelDialog",
      "EditFleetDialog",
      "EditModelDialog",
    ]);
    const productionFiles = [
      ...productionSourceFiles(resolve(process.cwd(), "app")),
      ...productionSourceFiles(resolve(process.cwd(), "components")),
    ];
    const eagerHeavyImports = productionFiles.flatMap((path) =>
      staticModuleSpecifiers(path)
        .filter((specifier) => {
          const moduleName = specifier
            .split("/")
            .at(-1)
            ?.replace(/\.tsx?$/, "");
          return moduleName ? heavyModules.has(moduleName) : false;
        })
        .map((specifier) => ({ path, specifier })),
    );
    expect(eagerHeavyImports).toEqual([]);

    const pending = deferred<{ default: string }>();
    const importModule = vi.fn(() => pending.promise);
    const loader = createIntentModuleLoader(importModule);

    expect(loader.getSnapshot()).toEqual({
      error: null,
      module: null,
      status: INTENT_MODULE_STATUS.idle,
    });
    expect(importModule).not.toHaveBeenCalled();

    const first = loader.preload();
    const second = loader.preload();
    expect(first).toBe(second);
    expect(importModule).toHaveBeenCalledTimes(1);
    expect(loader.getSnapshot().status).toBe(INTENT_MODULE_STATUS.loading);

    pending.resolve({ default: "loaded" });
    await expect(first).resolves.toEqual({ default: "loaded" });
    expect(loader.getSnapshot()).toEqual({
      error: null,
      module: { default: "loaded" },
      status: INTENT_MODULE_STATUS.ready,
    });

    await expect(loader.preload()).resolves.toEqual({ default: "loaded" });
    expect(importModule).toHaveBeenCalledTimes(1);
  });

  it("test_lazy_tool_failure_preserves_trigger_and_retry", async () => {
    const first = deferred<{ default: string }>();
    const second = deferred<{ default: string }>();
    const importModule = vi
      .fn<() => Promise<{ default: string }>>()
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const loader = createIntentModuleLoader(importModule);
    const states: string[] = [];
    const unsubscribe = loader.subscribe(() => {
      states.push(loader.getSnapshot().status);
    });

    const failed = loader.preload();
    first.reject(new Error("chunk unavailable"));
    await expect(failed).rejects.toThrow("chunk unavailable");
    expect(loader.getSnapshot().status).toBe(INTENT_MODULE_STATUS.error);
    expect(loader.getSnapshot().error).toBeInstanceOf(Error);

    const retried = loader.retry();
    expect(importModule).toHaveBeenCalledTimes(2);
    second.resolve({ default: "recovered" });
    await expect(retried).resolves.toEqual({ default: "recovered" });
    expect(loader.getSnapshot().status).toBe(INTENT_MODULE_STATUS.ready);
    expect(states).toEqual(["loading", "error", "loading", "ready"]);
    unsubscribe();
  });

  it("keeps a retry started by an error subscriber single-flight", async () => {
    const first = deferred<{ default: string }>();
    const second = deferred<{ default: string }>();
    const importModule = vi
      .fn<() => Promise<{ default: string }>>()
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const loader = createIntentModuleLoader(importModule);
    let retry: Promise<{ default: string }> | null = null;
    loader.subscribe(() => {
      if (loader.getSnapshot().status === INTENT_MODULE_STATUS.error) {
        retry = loader.retry();
      }
    });

    const failed = loader.preload();
    first.reject(new Error("chunk unavailable"));
    await expect(failed).rejects.toThrow("chunk unavailable");
    const concurrent = loader.preload();

    expect(importModule).toHaveBeenCalledTimes(2);
    expect(concurrent).toBe(retry);
    second.resolve({ default: "recovered" });
    await expect(concurrent).resolves.toEqual({ default: "recovered" });
    expect(loader.getSnapshot().status).toBe(INTENT_MODULE_STATUS.ready);
  });

  it("normalizes synchronous import failure and retries from every state", async () => {
    const failure = new Error("synchronous chunk failure");
    const failedLoader = createIntentModuleLoader<{ default: string }>(() => {
      throw failure;
    });
    await expect(failedLoader.preload()).rejects.toBe(failure);
    expect(failedLoader.getSnapshot()).toMatchObject({
      error: failure,
      status: INTENT_MODULE_STATUS.error,
    });

    const importModule = vi.fn(async () => ({ default: "loaded" }));
    const idleLoader = createIntentModuleLoader(importModule);
    await expect(idleLoader.retry()).resolves.toEqual({ default: "loaded" });
    expect(importModule).toHaveBeenCalledOnce();
  });

  it("publishes loader state through the React external-store hook", async () => {
    const pending = deferred<{ default: string }>();
    const loader = createIntentModuleLoader(() => pending.promise);
    const { result } = renderHook(() => useIntentModule(loader));
    expect(result.current.status).toBe(INTENT_MODULE_STATUS.idle);

    let request!: Promise<{ default: string }>;
    act(() => {
      request = loader.preload();
    });
    expect(result.current.status).toBe(INTENT_MODULE_STATUS.loading);

    await act(async () => {
      pending.resolve({ default: "hook-loaded" });
      await request;
    });
    expect(result.current).toMatchObject({
      module: { default: "hook-loaded" },
      status: INTENT_MODULE_STATUS.ready,
    });
  });
});

describe("intent capability policy", () => {
  it("test_intent_loading_respects_client_capabilities", () => {
    stubPointer(false);
    vi.stubGlobal("navigator", { connection: { saveData: false } });
    expect(maySpeculateOnHover()).toBe(true);

    stubPointer(true);
    expect(maySpeculateOnHover()).toBe(false);

    stubPointer(false);
    vi.stubGlobal("navigator", { connection: { saveData: true } });
    expect(maySpeculateOnHover()).toBe(false);
  });

  it("allows fine-pointer hover when the connection hint is absent", () => {
    stubPointer(false);
    vi.stubGlobal("navigator", {});
    expect(maySpeculateOnHover()).toBe(true);
  });

  it("allows hover when the browser has no navigator global", () => {
    stubPointer(false);
    vi.stubGlobal("navigator", undefined);
    expect(maySpeculateOnHover()).toBe(true);
  });

  it("refuses server-side speculation", () => {
    vi.stubGlobal("window", undefined);
    expect(maySpeculateOnHover()).toBe(false);
  });
});
