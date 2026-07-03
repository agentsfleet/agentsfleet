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
    expect(DesignSystem.DashboardPanel).toBeDefined();
    expect(DesignSystem.DashboardRow).toBeDefined();
    expect(DesignSystem.MetaGrid).toBeDefined();
    expect(DesignSystem.StatusPill).toBeDefined();
    expect(DesignSystem.Refpill).toBeDefined();
    expect(DesignSystem.TerminalPanel).toBeDefined();
    expect(DesignSystem.Terminal).toBeDefined();
    expect(DesignSystem.Grid).toBeDefined();
    expect(DesignSystem.Section).toBeDefined();
    expect(DesignSystem.InstallBlock).toBeDefined();
    expect(DesignSystem.WakePulse).toBeDefined();
  });

  it("exports utilities and variant helpers", () => {
    expect(DesignSystem.cn).toBeDefined();
    expect(DesignSystem.buttonVariants).toBeDefined();
    expect(DesignSystem.buttonClassName).toBeDefined();
  });

  it("exports the shared EYEBROW_CLASS eyebrow-typography constant", () => {
    expect(DesignSystem.EYEBROW_CLASS).toBeDefined();
    // It is the eyebrow token set — the single source every eyebrow composes.
    expect(DesignSystem.EYEBROW_CLASS).toContain("text-eyebrow");
    expect(DesignSystem.EYEBROW_CLASS).toContain("tracking-eyebrow");
    expect(DesignSystem.EYEBROW_CLASS).toContain("uppercase");
  });

  it.each([
    "Time",
    "List",
    "ListItem",
    "DescriptionList",
    "DescriptionTerm",
    "DescriptionDetails",
    "CopyButton",
  ] as const)("exports %s", (name) => {
    expect(DesignSystem[name]).toBeDefined();
  });
});
