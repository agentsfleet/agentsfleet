import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";

// The composer pulls in @assistant-ui/react; a light stub keeps the import
// factory's resolution deterministic.
vi.mock("@/components/domain/fleet/NewFleetChat", () => ({
  NewFleetChat: () => null,
}));

// Render the loading fallback first, then the mounted placeholder, as
// `next/dynamic` does with `ssr: false` once the chunk arrives.
vi.mock("next/dynamic", () => {
  type Loader = () => Promise<unknown>;
  type LoaderOpts = { loading?: () => React.ReactNode };
  return {
    default: (loader: Loader, opts: LoaderOpts) => function MockedDynamic(props: Record<string, unknown>) {
      const [ready, setReady] = React.useState(false);
      React.useEffect(() => {
        void loader();
        setReady(true);
      }, []);
      if (!ready && opts.loading) return opts.loading();
      return React.createElement("div", { "data-testid": "mounted-inner", ...props }, "inner");
    },
  };
});

import NewFleetChatDynamic from "@/components/domain/fleet/NewFleetChatDynamic";

afterEach(() => cleanup());

describe("NewFleetChatDynamic", () => {
  it("mounts the composer after the dynamic-load tick with the workspace it was given", async () => {
    const { findByTestId } = render(React.createElement(NewFleetChatDynamic, { workspace: "ws_prod", models: [] }));
    const inner = await findByTestId("mounted-inner");
    expect(inner.getAttribute("workspace")).toBe("ws_prod");
  });
});
