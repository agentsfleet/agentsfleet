import { it } from "vitest";
import { formatTimeRelative } from "./implementation";
import { checkRelative } from "../../helpers/check-relative";

const NOW = new Date("2026-05-03T12:00:00Z");

it("checks relative result", () => {
  const actual = formatTimeRelative(new Date(NOW.getTime() - 5_000), NOW);
  checkRelative(actual);
});
