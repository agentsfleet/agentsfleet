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
