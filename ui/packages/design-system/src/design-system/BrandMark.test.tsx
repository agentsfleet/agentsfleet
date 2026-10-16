import { expect, it } from "vitest";
import { render } from "@testing-library/react";
import { BrandMark } from "./BrandMark";

it("keeps brand identity static and out of the accessible wordmark name", () => {
  const { container } = render(<BrandMark className="size-5" />);
  const mark = container.firstElementChild!;
  expect(mark.getAttribute("aria-hidden")).toBe("true");
  expect(mark.getAttribute("focusable")).toBe("false");
  expect(mark.hasAttribute("data-live")).toBe(false);
  expect(mark.getAttribute("class")).toContain("size-5");
});
