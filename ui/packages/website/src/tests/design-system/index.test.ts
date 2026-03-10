import { describe, it, expect } from "vitest";
import * as DesignSystem from "@usezombie/design-system";

describe("design-system index exports", () => {
  it("exports all core components", () => {
    expect(DesignSystem.Button).toBeDefined();
    expect(DesignSystem.Card).toBeDefined();
    expect(DesignSystem.Terminal).toBeDefined();
    expect(DesignSystem.Grid).toBeDefined();
    expect(DesignSystem.Section).toBeDefined();
    expect(DesignSystem.InstallBlock).toBeDefined();
    expect(DesignSystem.AnimatedIcon).toBeDefined();
    expect(DesignSystem.ZombieHandIcon).toBeDefined();
  });
});
