import { expect, it } from "vitest";
import { cn } from "./implementation";

it("checks classes result", () => {
  const actual = cn("text-eyebrow", "text-muted-foreground");
  expect(cn("a", "b")).toBe("a b");
});
