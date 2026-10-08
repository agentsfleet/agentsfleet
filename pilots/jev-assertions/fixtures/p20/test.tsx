import { it } from "vitest";
import { formatElapsed } from "./implementation";
import { checkElapsed } from "../../helpers/check-elapsed";

it("checks elapsed result", () => {
  const actual = formatElapsed(59_600);
  checkElapsed(actual);
});
