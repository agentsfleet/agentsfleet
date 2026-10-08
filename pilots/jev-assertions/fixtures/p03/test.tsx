import { it } from "vitest";
import { cn } from "./implementation";
import { checkClasses } from "../../helpers/check-classes";

it("checks classes result", () => {
  const actual = cn("text-eyebrow", "text-muted-foreground");
  checkClasses(actual);
});
