import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, waitFor } from "@testing-library/react";

// Stub the heavy inner component (it pulls in @assistant-ui/react) so the
// dynamic import factory resolves to a light module.
vi.mock("@/components/domain/AgentThread", () => ({
  AgentThread: () => null,
}));

// `next/dynamic` lazy-loads the inner component asynchronously. Mock it
// to return a deterministic placeholder so the shim's mount path is
// covered without depending on the full Next.js runtime loader.
vi.mock("next/dynamic", () => {
  type Loader = () => Promise<unknown>;
  type LoaderOpts = { loading?: () => React.ReactNode };
  return {
    default: (loader: Loader, opts: LoaderOpts) => {
      // Render the configured loading fallback on first mount, then
      // resolve to a stub on the next tick. Mirrors next/dynamic's
      // ssr:false posture: server renders nothing/the loader,
      // client mounts the real component after hydration.
      return function MockedDynamic(props: Record<string, unknown>) {
        const [ready, setReady] = React.useState(false);
        React.useEffect(() => {
          // Exercise the real import factory (its `.then` mapper too); the
          // resolved module is irrelevant to this deterministic stub.
          void loader();
          setReady(true);
        }, []);
        if (!ready && opts.loading) return opts.loading();
        return React.createElement(
          "div",
          { "data-testid": "mounted-inner", ...props },
          "inner",
        );
      };
    },
  };
});

import AgentThreadDynamic from "@/components/domain/AgentThreadDynamic";

afterEach(() => cleanup());

describe("AgentThreadDynamic", () => {
  it("mounts the inner component after the dynamic-load tick", async () => {
    const { findByTestId } = render(
      React.createElement(AgentThreadDynamic, {
        workspaceId: "ws_1",
        agentId: "zomb_1",
        initial: [],
      }),
    );
    await waitFor(async () => {
      expect(await findByTestId("mounted-inner")).toBeTruthy();
    });
  });

  it("forwards workspace/agent props to the inner component", async () => {
    const { findByTestId } = render(
      React.createElement(AgentThreadDynamic, {
        workspaceId: "ws_prod",
        agentId: "zomb_42",
        initial: [],
      }),
    );
    const inner = await findByTestId("mounted-inner");
    expect(inner.getAttribute("workspaceid")).toBe("ws_prod");
    expect(inner.getAttribute("agentid")).toBe("zomb_42");
  });
});
