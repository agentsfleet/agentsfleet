import { expect, it } from "vitest";
import { cn } from "./implementation";

const COLOR_CLASS = "text-muted-foreground";

it("checks classes result", () => {
  const actual = cn("text-eyebrow", COLOR_CLASS);
  expect(actual).toContain(COLOR_CLASS);
});
