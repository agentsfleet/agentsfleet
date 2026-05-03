import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import {
  DescriptionList,
  DescriptionTerm,
  DescriptionDetails,
  dlVariants,
} from "./DescriptionList";

describe("DescriptionList", () => {
  it("renders <dl> with inline-layout flex utilities by default", () => {
    const { container } = render(
      <DescriptionList>
        <div>
          <DescriptionTerm>Mode</DescriptionTerm>
          <DescriptionDetails>platform</DescriptionDetails>
        </div>
      </DescriptionList>,
    );
    const dl = container.querySelector("dl");
    expect(dl).not.toBeNull();
    expect(dl?.className).toContain("[&>div]:flex");
    expect(dl?.className).toContain("[&>div]:justify-between");
  });

  it("renders <dl> without flex-row utilities when layout='stacked'", () => {
    const { container } = render(
      <DescriptionList layout="stacked">
        <DescriptionTerm>Mode</DescriptionTerm>
        <DescriptionDetails>platform</DescriptionDetails>
      </DescriptionList>,
    );
    const dl = container.querySelector("dl");
    expect(dl?.className).not.toContain("[&>div]:flex");
    expect(dl?.className).toContain("space-y-3");
  });

  it("DescriptionDetails with mono adds font-mono", () => {
    const { container } = render(
      <dl>
        <DescriptionDetails mono>abc-123</DescriptionDetails>
      </dl>,
    );
    expect(container.querySelector("dd")?.className).toContain("font-mono");
  });

  it("DescriptionDetails without mono omits font-mono", () => {
    const { container } = render(
      <dl>
        <DescriptionDetails>plain</DescriptionDetails>
      </dl>,
    );
    expect(container.querySelector("dd")?.className ?? "").not.toContain("font-mono");
  });

  it("DescriptionTerm renders <dt> with muted-foreground", () => {
    const { container } = render(
      <dl>
        <DescriptionTerm>Mode</DescriptionTerm>
      </dl>,
    );
    expect(container.querySelector("dt")?.className).toContain("text-muted-foreground");
  });

  it("dlVariants returns the inline token string", () => {
    expect(dlVariants({ layout: "inline" })).toContain("[&>div]:justify-between");
  });

  it("inline contract: bare <DescriptionTerm>/<DescriptionDetails> children DO NOT pick up row layout (documented requirement)", () => {
    // Bug this catches: a future caller writes <DescriptionList><Term/><Details/>
    // </DescriptionList> without the <div> wrapper, expecting the inline flex
    // layout, and gets a silently broken UI. This test pins the contract: the
    // utility selector targets direct <div> children, so bare children land
    // outside the flex row groups by design. The dlVariants inline string
    // includes the `[&>div]` selector, but with no <div> in the tree there is
    // nothing for it to match. The visible failure is layout, not classes —
    // assert the structural state instead.
    const { container } = render(
      <DescriptionList>
        <DescriptionTerm data-testid="bare-dt">Mode</DescriptionTerm>
        <DescriptionDetails data-testid="bare-dd">platform</DescriptionDetails>
      </DescriptionList>,
    );
    const dl = container.querySelector("dl") as HTMLElement;
    // Direct children are <dt>/<dd>, not <div> — the [&>div] selector matches
    // nothing, so no flex row wraps them.
    const directChildren = Array.from(dl.children).map((c) => c.tagName);
    expect(directChildren).toEqual(["DT", "DD"]);
    expect(directChildren.includes("DIV")).toBe(false);
  });

  it("inline contract: <div>-wrapped pairs DO render as direct <div> children that the [&>div] selector targets", () => {
    const { container } = render(
      <DescriptionList>
        <div>
          <DescriptionTerm>Mode</DescriptionTerm>
          <DescriptionDetails>platform</DescriptionDetails>
        </div>
        <div>
          <DescriptionTerm>Provider</DescriptionTerm>
          <DescriptionDetails>openai</DescriptionDetails>
        </div>
      </DescriptionList>,
    );
    const dl = container.querySelector("dl") as HTMLElement;
    const directChildren = Array.from(dl.children).map((c) => c.tagName);
    expect(directChildren).toEqual(["DIV", "DIV"]);
  });

  it("merges a custom className on the dl", () => {
    const { container } = render(
      <DescriptionList className="custom-dl">
        <div>
          <DescriptionTerm>k</DescriptionTerm>
          <DescriptionDetails>v</DescriptionDetails>
        </div>
      </DescriptionList>,
    );
    expect(container.querySelector("dl")?.className).toContain("custom-dl");
  });
});
