import React from "react";
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

import {
  LOADING_VERBS,
  loadingAccessibleName,
  loadingPhrase,
  pickLoadingVerb,
} from "../components/layout/loading-verbs";

// A loader shows a random waiting verb ("Wrangling Fleets…") instead of a static
// "Loading". Two things must hold no matter which verb wins: the visible phrase
// still reads as English in both the titled and title-less slots, and the
// announced name stays the plain wording so assistive tech never reads whimsy.

describe("loading verb vocabulary", () => {
  it("offers enough verbs that repeats are not the common case", () => {
    expect(LOADING_VERBS.length).toBeGreaterThanOrEqual(10);
  });

  it("has no duplicates", () => {
    expect(new Set(LOADING_VERBS).size).toBe(LOADING_VERBS.length);
  });

  it("is uniformly present-participle and capitalised so both slots read as English", () => {
    for (const verb of LOADING_VERBS) {
      // "Wrangling Fleets…" and a bare "Wrangling…" must both parse; a verb that
      // needs an object (e.g. "Fetching") strands the title-less dashboard slot.
      expect(verb).toMatch(/^[A-Z][a-z]+ing$/);
    }
  });
});

describe("pickLoadingVerb", () => {
  it("only ever returns a member of the vocabulary", () => {
    for (let i = 0; i < 200; i += 1) {
      expect(LOADING_VERBS).toContain(pickLoadingVerb());
    }
  });

  it("can reach every verb — no entry is unreachable off-by-one", () => {
    // Guards the Math.floor(random * length) indexing: an off-by-one would make
    // either the first or last verb permanently unreachable.
    const seen = new Set<string>();
    for (let i = 0; i < 5000; i += 1) seen.add(pickLoadingVerb());
    expect(seen.size).toBe(LOADING_VERBS.length);
  });

  it("returns the last verb at the top of the random range, never undefined", () => {
    // Math.random() is documented as [0,1); 0.999… must index the final entry.
    const random = vi.spyOn(Math, "random").mockReturnValue(0.9999999999);
    expect(pickLoadingVerb()).toBe(LOADING_VERBS[LOADING_VERBS.length - 1]);
    random.mockReturnValue(0);
    expect(pickLoadingVerb()).toBe(LOADING_VERBS[0]);
    random.mockRestore();
  });

  it("degrades to the first verb if the index ever lands out of range", () => {
    // A hostile mock returning exactly 1 (out of Math.random's spec) drives the
    // index past the array — the total-function fallback must yield a real verb,
    // never undefined leaking into the rendered phrase.
    const random = vi.spyOn(Math, "random").mockReturnValue(1);
    expect(pickLoadingVerb()).toBe(LOADING_VERBS[0]);
    random.mockRestore();
  });
});

describe("loadingPhrase", () => {
  it("names the route when there is a title", () => {
    expect(loadingPhrase("Wrangling", "Fleets")).toBe("Wrangling Fleets…");
  });

  it("stands alone when the fallback covers many routes", () => {
    // The workspace-home loader is title-less on purpose — it must not invent a
    // route name, and must not leave a dangling space before the ellipsis.
    expect(loadingPhrase("Wrangling")).toBe("Wrangling…");
    expect(loadingPhrase("Wrangling", "")).toBe("Wrangling…");
  });
});

describe("loadingAccessibleName", () => {
  it("stays plain so screen readers never announce the whimsy", () => {
    expect(loadingAccessibleName("Fleets")).toBe("Loading Fleets");
    expect(loadingAccessibleName()).toBe("Loading");
  });

  it("contains no verb from the vocabulary", () => {
    const name = loadingAccessibleName("Fleets");
    for (const verb of LOADING_VERBS) {
      expect(name).not.toContain(verb);
    }
  });
});

describe("LoadingVerbLabel", () => {
  it("freezes the verb at mount so the word never changes mid-wait", async () => {
    const { LoadingVerbLabel } = await import("../components/layout/LoadingVerbLabel");
    const { render } = await import("@testing-library/react");

    const { container, rerender } = render(
      React.createElement(LoadingVerbLabel, { title: "Fleets" }),
    );
    const first = container.textContent;
    // A re-render must not re-roll: a rotating word would retext the role=status
    // live region and make assistive tech re-announce mid-wait.
    rerender(React.createElement(LoadingVerbLabel, { title: "Fleets" }));
    expect(container.textContent).toBe(first);
  });

  it("does not log a hydration mismatch when server and client pick different verbs", async () => {
    const { LoadingVerbLabel } = await import("../components/layout/LoadingVerbLabel");
    const { renderToString } = await import("react-dom/server");
    const { hydrateRoot } = await import("react-dom/client");
    const { act } = await import("react");

    // Server renders the FIRST verb, client is forced to pick the LAST — the
    // exact disagreement suppressHydrationWarning exists to cover.
    const random = vi.spyOn(Math, "random").mockReturnValue(0);
    const serverHtml = renderToString(
      React.createElement(LoadingVerbLabel, { title: "Fleets" }),
    );
    random.mockReturnValue(0.9999999999);

    const container = document.createElement("div");
    container.innerHTML = serverHtml;
    document.body.appendChild(container);

    const errors: string[] = [];
    // String(), not JSON.stringify: React passes an Error object, which
    // serialises to "{}" and would silently swallow the message this asserts on.
    const spy = vi
      .spyOn(console, "error")
      .mockImplementation((...a) => errors.push(a.map(String).join(" ")));
    await act(async () => {
      hydrateRoot(container, React.createElement(LoadingVerbLabel, { title: "Fleets" }));
    });
    spy.mockRestore();
    random.mockRestore();

    expect(errors.filter((e) => /hydrat|did not match|mismatch/i.test(e))).toEqual([]);
    // Non-vacuity guard: without the escape hatch React regenerates the subtree
    // to the client's word. Asserting the server's word survived proves the
    // suppression is actually in force, not that the check simply found nothing.
    expect(container.textContent).toBe(`${LOADING_VERBS[0]} Fleets…`);
    document.body.removeChild(container);
  });
});

describe("RouteLoading integration", () => {
  it("renders a real verb, keeps the title, and pins the announced name", async () => {
    const { default: RouteLoading } = await import("../components/layout/RouteLoading");
    const markup = renderToStaticMarkup(
      React.createElement(RouteLoading, { title: "Fleets", description: "d" }),
    );

    expect(markup).toContain("Fleets");
    expect(markup).toContain('aria-label="Loading Fleets"');
    // Exactly one verb should be present, and the old static copy should be gone
    // from the visible text.
    const present = LOADING_VERBS.filter((v) => markup.includes(`${v} Fleets…`));
    expect(present).toHaveLength(1);
    expect(markup).not.toContain("Loading Fleets…");
  });

  it("keeps the title-less dashboard fallback free of a route name", async () => {
    const { default: DashboardLoading } = await import(
      "../app/(dashboard)/w/[workspaceId]/loading"
    );
    const markup = renderToStaticMarkup(React.createElement(DashboardLoading));

    expect(markup).toContain('aria-label="Loading"');
    const present = LOADING_VERBS.filter((v) => markup.includes(`${v}…`));
    expect(present).toHaveLength(1);
  });
});
