import { describe, it, expect } from "vitest";
import * as DesignSystem from "./index";

describe("design-system public exports", () => {
  it("exports every core component", () => {
    expect(DesignSystem.Button).toBeDefined();
    expect(DesignSystem.Card).toBeDefined();
    expect(DesignSystem.CardHeader).toBeDefined();
    expect(DesignSystem.CardTitle).toBeDefined();
    expect(DesignSystem.CardDescription).toBeDefined();
    expect(DesignSystem.CardContent).toBeDefined();
    expect(DesignSystem.CardFooter).toBeDefined();
    expect(DesignSystem.Terminal).toBeDefined();
    expect(DesignSystem.Grid).toBeDefined();
    expect(DesignSystem.Section).toBeDefined();
    expect(DesignSystem.InstallBlock).toBeDefined();
    expect(DesignSystem.AnimatedIcon).toBeDefined();
    expect(DesignSystem.ZombieHandIcon).toBeDefined();
  });

  it("exports utilities and variant helpers", () => {
    expect(DesignSystem.cn).toBeDefined();
    expect(DesignSystem.buttonVariants).toBeDefined();
    expect(DesignSystem.buttonClassName).toBeDefined();
  });

  it.each([
    "Time",
    "List",
    "ListItem",
    "DescriptionList",
    "DescriptionTerm",
    "DescriptionDetails",
  ] as const)("exports %s", (name) => {
    expect(DesignSystem[name]).toBeDefined();
  });
});
