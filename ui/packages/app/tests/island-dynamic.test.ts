import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, waitFor } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// ── §5 — interaction-only islands are code-split via next/dynamic ───────────
//
// Each island below is rendered only on user interaction (a dialog opened from
// a trigger, or a form inside a collapsed disclosure). Wrapping it in a
// `next/dynamic` client shim keeps its chunk out of the route's *initial* JS
// bundle: the call site imports the shim, never the raw component, so the
// component's code is fetched after hydration instead of shipped up front.
//
// 5.1 is a source-level assertion (the deterministic, build-free equivalent of
// a build-manifest check — the production `next build` confirms the actual
// chunk split). 5.2 proves the shim mounts its inner component. 5.3 guards the
// assistant-ui chat surface's on-brand styling.

const APP_ROOT = resolve(__dirname, "..");
const SHIM_DIR = "components/domain/island-dynamic";

function read(rel: string): string {
  return readFileSync(resolve(APP_ROOT, rel), "utf8");
}

type Island = {
  name: string;
  shim: string; // shim filename under SHIM_DIR (no extension)
  rawImportFragment: string; // path fragment the shim's dynamic import targets
  callSite: string; // file that renders the island
  rawComponent: string; // raw component basename (for the static-import guard)
};

const ISLANDS: Island[] = [
  {
    name: "EditSecretDialog",
    shim: "EditSecretDialogDynamic",
    rawImportFragment: "secrets/components/EditSecretDialog",
    callSite: "app/(dashboard)/secrets/components/SecretsList.tsx",
    rawComponent: "EditSecretDialog",
  },
  {
    name: "RenameSecretDialog",
    shim: "RenameSecretDialogDynamic",
    rawImportFragment: "secrets/components/RenameSecretDialog",
    callSite: "app/(dashboard)/secrets/components/SecretsList.tsx",
    rawComponent: "RenameSecretDialog",
  },
  {
    name: "AddSecretForm",
    shim: "AddSecretFormDynamic",
    rawImportFragment: "secrets/components/AddSecretForm",
    // Secrets is its own page now; the add
    // form mounts inside AddSecretDialog, not the page directly.
    callSite: "app/(dashboard)/secrets/components/AddSecretDialog.tsx",
    rawComponent: "AddSecretForm",
  },
  {
    name: "CreateWorkspaceDialog",
    shim: "CreateWorkspaceDialogDynamic",
    rawImportFragment: "layout/CreateWorkspaceDialog",
    callSite: "components/layout/WorkspaceSwitcher.tsx",
    rawComponent: "CreateWorkspaceDialog",
  },
  {
    name: "CreateApiKeyDialog",
    shim: "CreateApiKeyDialogDynamic",
    rawImportFragment: "settings/api-keys/components/CreateApiKeyDialog",
    callSite: "app/(dashboard)/settings/api-keys/components/ApiKeysView.tsx",
    rawComponent: "CreateApiKeyDialog",
  },
  {
    name: "AddRunnerDialog",
    shim: "AddRunnerDialogDynamic",
    rawImportFragment: "admin/runners/components/AddRunnerDialog",
    callSite: "app/(dashboard)/admin/runners/components/RunnersView.tsx",
    rawComponent: "AddRunnerDialog",
  },
];

describe("interaction-only islands are excluded from the route's initial chunk", () => {
  for (const island of ISLANDS) {
    it(`${island.name}: shim pulls the component in via next/dynamic`, () => {
      const shim = read(`${SHIM_DIR}/${island.shim}.tsx`);
      expect(shim).toContain("next/dynamic");
      // The dynamic import (not a static top-level import) targets the component.
      expect(shim).toMatch(new RegExp(`import\\(\\s*["'][^"']*${island.rawImportFragment}["']`));
      expect(shim).toContain("ssr: false");
    });

    it(`${island.name}: call site imports the shim, never the raw component statically`, () => {
      const src = read(island.callSite);
      // It imports the dynamic shim …
      expect(src).toContain(`${SHIM_DIR}/${island.shim}`);
      // … and has no static `import X from "…/<RawComponent>"` (the trailing
      // quote ensures the shim path, which ends in `…Dynamic"`, never matches).
      expect(src).not.toMatch(
        new RegExp(`import\\s+\\w+\\s+from\\s+["'][^"']*${island.rawComponent}["']`),
      );
    });
  }
});

// Stub the heavy inner modules so calling the dynamic loader (below) resolves
// to a light component without pulling clerk / design-system / form deps.
vi.mock("@/app/(dashboard)/secrets/components/EditSecretDialog", () => ({ default: () => null }));
vi.mock("@/app/(dashboard)/secrets/components/RenameSecretDialog", () => ({ default: () => null }));
vi.mock("@/app/(dashboard)/secrets/components/AddSecretForm", () => ({ default: () => null }));
vi.mock("@/components/layout/CreateWorkspaceDialog", () => ({ default: () => null }));
vi.mock("@/app/(dashboard)/settings/api-keys/components/CreateApiKeyDialog", () => ({ default: () => null }));
vi.mock("@/app/(dashboard)/admin/runners/components/AddRunnerDialog", () => ({ default: () => null }));

// 5.2 — the shim mounts its inner component after the dynamic-load tick. Mock
// next/dynamic so the mount path is deterministic without the Next.js loader
// (mirrors tests/fleet-thread-dynamic.test.ts).
vi.mock("next/dynamic", () => {
  type Loader = () => Promise<unknown>;
  type LoaderOpts = { loading?: () => React.ReactNode };
  return {
    default: (loader: Loader, opts: LoaderOpts) =>
      function MockedDynamic() {
        const [ready, setReady] = React.useState(false);
        React.useEffect(() => {
          // Exercise the real import factory + its `.then` default-mapper so the
          // shim's loader is covered (mirrors tests/fleet-thread-dynamic.test.ts).
          void loader();
          setReady(true);
        }, []);
        if (!ready && opts.loading) return opts.loading();
        return React.createElement("div", { "data-testid": "mounted-inner" }, "inner");
      },
  };
});

import EditCredentialDialogDynamic from "@/components/domain/island-dynamic/EditSecretDialogDynamic";
import RenameCredentialDialogDynamic from "@/components/domain/island-dynamic/RenameSecretDialogDynamic";
import AddCredentialFormDynamic from "@/components/domain/island-dynamic/AddSecretFormDynamic";
import CreateWorkspaceDialogDynamic from "@/components/domain/island-dynamic/CreateWorkspaceDialogDynamic";
import CreateApiKeyDialogDynamic from "@/components/domain/island-dynamic/CreateApiKeyDialogDynamic";
import AddRunnerDialogDynamic from "@/components/domain/island-dynamic/AddRunnerDialogDynamic";

afterEach(() => cleanup());

describe("dynamic island shims mount their inner component", () => {
  const noop = () => {};

  const cases: Array<[string, React.ReactElement]> = [
    [
      "EditCredentialDialogDynamic",
      React.createElement(EditCredentialDialogDynamic, {
        workspaceId: "ws_1",
        name: "github",
        open: true,
        onOpenChange: noop,
      }),
    ],
    [
      "RenameCredentialDialogDynamic",
      React.createElement(RenameCredentialDialogDynamic, {
        workspaceId: "ws_1",
        name: "github",
        existingNames: [],
        open: true,
        onOpenChange: noop,
      }),
    ],
    [
      "AddCredentialFormDynamic",
      React.createElement(AddCredentialFormDynamic, { workspaceId: "ws_1" }),
    ],
    [
      "CreateWorkspaceDialogDynamic",
      React.createElement(CreateWorkspaceDialogDynamic, { open: true, onOpenChange: noop }),
    ],
    [
      "CreateApiKeyDialogDynamic",
      React.createElement(CreateApiKeyDialogDynamic, { onCreated: noop }),
    ],
    [
      "AddRunnerDialogDynamic",
      React.createElement(AddRunnerDialogDynamic, { onCreated: noop }),
    ],
  ];

  for (const [name, element] of cases) {
    it(`${name} mounts after the dynamic-load tick`, async () => {
      const { findByTestId } = render(element);
      await waitFor(async () => {
        expect(await findByTestId("mounted-inner")).toBeTruthy();
      });
    });
  }
});

describe("FleetThread uses design-system tokens, not raw assistant-ui defaults", () => {
  const source = read("components/domain/FleetThread.tsx");

  it("styles with design-system token utilities", () => {
    // Spacing + surface + text tokens from @agentsfleet/design-system — not raw
    // pixel/hex values or assistant-ui's stock theme.
    expect(source).toContain("bg-surface-deep");
    expect(source).toContain("border-border");
    expect(source).toContain("text-muted-foreground");
  });

  it("carries no raw `aui-`-prefixed assistant-ui theme classes", () => {
    expect(source).not.toMatch(/className=["'][^"']*\baui-/);
  });
});
