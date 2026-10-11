import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, waitFor } from "@testing-library/react";

// Interaction-only islands load through a `next/dynamic` shim. Each shim's
// loader must resolve to the right inner component and hand it every prop the
// call site passed; a `.then` mapper reading the wrong export, or a shim that
// drops props, renders nothing or the wrong thing here.

const { INNER_ATTR, PROPS_ATTR, innerStub } = vi.hoisted(() => {
  const INNER_ATTR = "data-inner";
  const PROPS_ATTR = "data-prop-keys";
  type CreateElement = (type: string, props: Record<string, string>) => unknown;
  // Each stub names the module it stands in for and lists the props it got.
  const innerStub = (name: string, createElement: CreateElement) =>
    function InnerStub(props: Record<string, unknown>) {
      return createElement("div", { [INNER_ATTR]: name, [PROPS_ATTR]: Object.keys(props).sort().join(",") });
    };
  return { INNER_ATTR, PROPS_ATTR, innerStub };
});

// Each stub keeps the real module's export shape: every inner is a default
// export except EditPolicyDialog, which is named.
vi.mock("@/app/(dashboard)/w/[workspaceId]/secrets/components/EditSecretDialog", async () => ({
  default: innerStub("EditSecretDialog", (await import("react")).createElement),
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/secrets/components/RenameSecretDialog", async () => ({
  default: innerStub("RenameSecretDialog", (await import("react")).createElement),
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/secrets/components/AddSecretForm", async () => ({
  default: innerStub("AddSecretForm", (await import("react")).createElement),
}));
vi.mock("@/components/layout/CreateWorkspaceDialog", async () => ({
  default: innerStub("CreateWorkspaceDialog", (await import("react")).createElement),
}));
vi.mock("@/app/(dashboard)/settings/api-keys/components/CreateApiKeyDialog", async () => ({
  default: innerStub("CreateApiKeyDialog", (await import("react")).createElement),
}));
vi.mock("@/app/(dashboard)/settings/members/components/InviteDialog", async () => ({
  default: innerStub("InviteDialog", (await import("react")).createElement),
}));
vi.mock("@/app/(dashboard)/admin/runners/components/AddRunnerDialog", async () => ({
  default: innerStub("AddRunnerDialog", (await import("react")).createElement),
}));
vi.mock("@/app/(dashboard)/admin/runners/components/EditPolicyDialog", async () => ({
  EditPolicyDialog: innerStub("EditPolicyDialog", (await import("react")).createElement),
}));

// A deterministic stand-in for the Next.js loader: await the shim's own
// factory and render whatever component it resolves to, with the props given.
vi.mock("next/dynamic", () => {
  type Loaded = { default: React.ComponentType<Record<string, unknown>> };
  type LoaderOpts = { loading?: () => React.ReactNode };
  return {
    default: (loader: () => Promise<Loaded>, opts: LoaderOpts) =>
      function MockedDynamic(props: Record<string, unknown>) {
        const [inner, setInner] = React.useState<Loaded["default"] | null>(null);
        React.useEffect(() => {
          void loader().then((mod) => setInner(() => mod.default));
        }, []);
        if (!inner) return opts.loading ? opts.loading() : null;
        return React.createElement(inner, props);
      },
  };
});

import EditCredentialDialogDynamic from "@/components/domain/island-dynamic/EditSecretDialogDynamic";
import RenameCredentialDialogDynamic from "@/components/domain/island-dynamic/RenameSecretDialogDynamic";
import AddCredentialFormDynamic from "@/components/domain/island-dynamic/AddSecretFormDynamic";
import CreateWorkspaceDialogDynamic from "@/components/domain/island-dynamic/CreateWorkspaceDialogDynamic";
import CreateApiKeyDialogDynamic from "@/components/domain/island-dynamic/CreateApiKeyDialogDynamic";
import InviteDialogDynamic from "@/components/domain/island-dynamic/InviteDialogDynamic";
import AddRunnerDialogDynamic from "@/components/domain/island-dynamic/AddRunnerDialogDynamic";
import EditPolicyDialogDynamic from "@/components/domain/island-dynamic/EditPolicyDialogDynamic";

afterEach(() => cleanup());

describe("dynamic island shims load their own inner component with the caller's props", () => {
  const noop = () => {};

  const cases: Array<[string, string, React.ReactElement]> = [
    [
      "EditCredentialDialogDynamic",
      "EditSecretDialog",
      React.createElement(EditCredentialDialogDynamic, {
        workspaceId: "ws_1",
        name: "github",
        open: true,
        onOpenChange: noop,
      }),
    ],
    [
      "RenameCredentialDialogDynamic",
      "RenameSecretDialog",
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
      "AddSecretForm",
      React.createElement(AddCredentialFormDynamic, { workspaceId: "ws_1" }),
    ],
    [
      "CreateWorkspaceDialogDynamic",
      "CreateWorkspaceDialog",
      React.createElement(CreateWorkspaceDialogDynamic, {
        open: true,
        pending: false,
        error: null,
        onOpenChange: noop,
        onSubmit: noop,
      }),
    ],
    [
      "CreateApiKeyDialogDynamic",
      "CreateApiKeyDialog",
      React.createElement(CreateApiKeyDialogDynamic, { onCreated: noop }),
    ],
    [
      "InviteDialogDynamic",
      "InviteDialog",
      React.createElement(InviteDialogDynamic, { onSettled: noop }),
    ],
    [
      "AddRunnerDialogDynamic",
      "AddRunnerDialog",
      React.createElement(AddRunnerDialogDynamic, { onCreated: noop }),
    ],
    [
      "EditPolicyDialogDynamic",
      "EditPolicyDialog",
      React.createElement(EditPolicyDialogDynamic, {
        runnerId: "run_1",
        current: null,
        onSaved: noop,
      }),
    ],
  ];

  for (const [shim, inner, element] of cases) {
    it(`${shim} renders ${inner} with every prop it was given`, async () => {
      const { container } = render(element);
      await waitFor(() => {
        expect(container.querySelector(`[${INNER_ATTR}]`)?.getAttribute(INNER_ATTR)).toBe(inner);
      });
      const given = Object.keys(element.props as Record<string, unknown>).sort().join(",");
      expect(container.querySelector(`[${INNER_ATTR}]`)?.getAttribute(PROPS_ATTR)).toBe(given);
    });
  }
});
