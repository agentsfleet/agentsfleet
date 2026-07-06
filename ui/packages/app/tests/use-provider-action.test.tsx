import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";

// The shared action-runner behind every Models mutation: clear error, flip
// pending, await a server action returning a ProviderActionError | null; on
// null run an optional success step + router.refresh(); on an error, route it
// through presentErrorString so the raw backend string never
// reaches the UI unmapped.

const routerRefresh = vi.fn();
vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: routerRefresh, push: vi.fn() }) }));

import { useProviderAction, type ProviderActionError } from "@/app/(dashboard)/w/[workspaceId]/settings/models/lib/use-provider-action";

const ACTION = "do the thing";

type ProbeProps = {
  fn: () => Promise<ProviderActionError | null>;
  onSuccess?: () => void;
};

function Probe({ fn, onSuccess }: ProbeProps) {
  const { pending, error, setError, run } = useProviderAction();
  return React.createElement(
    "div",
    null,
    React.createElement("span", { "data-testid": "pending" }, String(pending)),
    React.createElement("span", { "data-testid": "error" }, error ?? ""),
    React.createElement("button", { "data-testid": "run", onClick: () => run(ACTION, fn, onSuccess) }, "run"),
    React.createElement("button", { "data-testid": "seterr", onClick: () => setError("manual") }, "seterr"),
  );
}

beforeEach(() => vi.clearAllMocks());
afterEach(() => cleanup());

describe("useProviderAction", () => {
  it("runs the success path: onSuccess + router.refresh, error cleared, pending toggled", async () => {
    const onSuccess = vi.fn();
    let resolveFn!: (v: ProviderActionError | null) => void;
    const fn = vi.fn(() => new Promise<ProviderActionError | null>((r) => (resolveFn = r)));
    render(React.createElement(Probe, { fn, onSuccess }));

    act(() => {
      screen.getByTestId("run").click();
    });
    // pending flips true while the action is in flight.
    await waitFor(() => expect(screen.getByTestId("pending").textContent).toBe("true"));

    await act(async () => {
      resolveFn(null);
    });

    expect(onSuccess).toHaveBeenCalledTimes(1);
    expect(routerRefresh).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("error").textContent).toBe("");
    expect(screen.getByTestId("pending").textContent).toBe("false");
  });

  it("routes the error through presentErrorString and skips onSuccess + refresh", async () => {
    const onSuccess = vi.fn();
    render(
      React.createElement(Probe, { fn: () => Promise.resolve({ message: "boom" }), onSuccess }),
    );
    await act(async () => {
      screen.getByTestId("run").click();
    });
    await waitFor(() => expect(screen.getByTestId("error").textContent).toMatch(/^Couldn't do the thing/));
    expect(screen.getByTestId("error").textContent).toMatch(/boom/);
    expect(onSuccess).not.toHaveBeenCalled();
    expect(routerRefresh).not.toHaveBeenCalled();
    expect(screen.getByTestId("pending").textContent).toBe("false");
  });

  it("maps a curated errorCode to its friendly copy instead of the raw message", async () => {
    render(
      React.createElement(Probe, {
        fn: () => Promise.resolve({ message: "raw backend detail", errorCode: "UZ-AUTH-022" }),
      }),
    );
    await act(async () => {
      screen.getByTestId("run").click();
    });
    await waitFor(() =>
      expect(screen.getByTestId("error").textContent).toBe(
        "You need an additional scope for that. Ask an agentsfleet admin to grant the scope this action requires.",
      ),
    );
  });

  it("refreshes even without an onSuccess callback", async () => {
    render(React.createElement(Probe, { fn: () => Promise.resolve(null) }));
    await act(async () => {
      screen.getByTestId("run").click();
    });
    await waitFor(() => expect(routerRefresh).toHaveBeenCalledTimes(1));
  });

  it("exposes setError for callers to set an error directly", async () => {
    render(React.createElement(Probe, { fn: () => Promise.resolve(null) }));
    await act(async () => {
      screen.getByTestId("seterr").click();
    });
    await waitFor(() => expect(screen.getByTestId("error").textContent).toBe("manual"));
  });
});
