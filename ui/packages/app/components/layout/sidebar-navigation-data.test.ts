import {expect, it, vi} from "vitest";
import {primaryNavigation} from "./sidebar-navigation-data";

const feature = vi.hoisted(() => ({enabled: false}));
vi.mock("@/lib/plugin-ui", () => ({pluginsUiEnabled: () => feature.enabled}));

it.each([false, true])("keeps the Plugin entry reversible through the one UI switch (%s)", enabled => {
  feature.enabled = enabled;
  const names = primaryNavigation().map(entry => entry.label);
  expect(names.includes("Plugins")).toBe(enabled);
  expect(names).toContain("Your fleets");expect(names).toContain("Connected apps");
});
