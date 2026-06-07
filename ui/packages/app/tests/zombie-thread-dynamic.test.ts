import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, waitFor } from "@testing-library/react";

// Stub the heavy inner component (it pulls in @assistant-ui/react) so the
// dynamic import factory resolves to a light module.
vi.mock("@/components/domain/ZombieThread", () => ({
  ZombieThread: () => null,
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

import ZombieThreadDynamic from "@/components/domain/ZombieThreadDynamic";

afterEach(() => cleanup());

describe("ZombieThreadDynamic", () => {
  it("mounts the inner component after the dynamic-load tick", async () => {
    const { findByTestId } = render(
      React.createElement(ZombieThreadDynamic, {
        workspaceId: "ws_1",
        zombieId: "zomb_1",
        initial: [],
      }),
    );
    await waitFor(async () => {
      expect(await findByTestId("mounted-inner")).toBeTruthy();
    });
  });

  it("forwards workspace/zombie props to the inner component", async () => {
    const { findByTestId } = render(
      React.createElement(ZombieThreadDynamic, {
        workspaceId: "ws_prod",
        zombieId: "zomb_42",
        initial: [],
      }),
    );
    const inner = await findByTestId("mounted-inner");
    expect(inner.getAttribute("workspaceid")).toBe("ws_prod");
    expect(inner.getAttribute("zombieid")).toBe("zomb_42");
  });
});
