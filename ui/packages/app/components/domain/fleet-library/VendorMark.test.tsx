/**
 * A credential is drawn as its provider's mark, or as nothing pretending to be
 * one.
 *
 * The fallback is the case worth holding. Simple Icons ships no Slack mark —
 * dropped on a trademark request — and Slack is in this product's own
 * fixtures, so "the provider has no mark" is an ordinary Tuesday rather than
 * an edge case. It has to render something deliberate, and it must never
 * render a DIFFERENT provider's mark.
 */
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";

import { VendorMark } from "./VendorMark";
import { VENDOR_MARKS, vendorMark } from "./vendor-marks";

afterEach(cleanup);

describe("a credential draws its provider's mark", () => {
  it("draws the mark for a provider the map knows", () => {
    const { container } = render(<VendorMark credential="github" />);

    const svg = container.querySelector("[data-vendor-mark='github']");
    expect(svg).toBeTruthy();
    expect(svg?.querySelector("path")?.getAttribute("d")).toBe(VENDOR_MARKS.github?.path);
  });

  it("inherits colour and size rather than carrying its own", () => {
    // A mark that hard-codes a brand colour fights every surface it lands on,
    // and one that hard-codes a size cannot sit in a row with the others.
    const { container } = render(<VendorMark credential="grafana" />);

    const svg = container.querySelector("[data-vendor-mark='grafana']");
    expect(svg?.getAttribute("fill")).toBe("currentColor");
    expect(svg?.getAttribute("class")).toContain("size-4");
  });

  it("draws a neutral glyph for a provider with no mark", () => {
    // Slack, specifically: it is in the fixtures and it has no mark upstream.
    const { container } = render(<VendorMark credential="slack" />);

    expect(container.querySelector("[data-vendor-mark]")).toBeNull();
    expect(container.querySelector("svg")).toBeTruthy();
  });

  it("never substitutes another provider's mark for an unknown one", () => {
    // The failure this guards is a lookup that falls through to a default
    // entry: a bundle needing Datadog must not appear to need GitHub.
    const { container } = render(<VendorMark credential="datadog" />);

    for (const known of Object.keys(VENDOR_MARKS)) {
      expect(container.querySelector(`[data-vendor-mark='${known}']`)).toBeNull();
    }
  });

  it("is hidden from the accessibility tree, because the row names it", () => {
    // The mark is decoration over a tooltip that carries every credential by
    // name. Announcing the glyph too would read the same fact twice, and for
    // the neutral one it would announce a fact it does not carry.
    const { container } = render(<VendorMark credential="github" />);

    expect(container.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
  });
});

describe("the lookup refuses inherited properties", () => {
  it("falls through for a credential named after an Object member", () => {
    // Credential names arrive from bundle frontmatter, which anyone can write.
    // A plain index would resolve `constructor` to a function and render it.
    for (const hostile of ["constructor", "toString", "__proto__", "hasOwnProperty"]) {
      expect(vendorMark(hostile)).toBeUndefined();
    }
  });

  it("resolves a credential the map does hold", () => {
    expect(vendorMark("elastic")?.title).toBe("Elastic");
  });
});

describe("every vendored mark is usable as drawn", () => {
  it("carries a title and a path", () => {
    for (const [credential, mark] of Object.entries(VENDOR_MARKS)) {
      expect(mark.title, `${credential} has no title`).toBeTruthy();
      expect(mark.path.length, `${credential} has no path`).toBeGreaterThan(0);
    }
  });

  it("carries no fill of its own, so it can inherit one", () => {
    // Simple Icons paths are bare geometry. One arriving with `fill="#181717"`
    // baked in would ignore `currentColor` and render black on a dark card.
    for (const [credential, mark] of Object.entries(VENDOR_MARKS)) {
      expect(mark.path, `${credential} carries a fill`).not.toContain("fill");
    }
  });
});
