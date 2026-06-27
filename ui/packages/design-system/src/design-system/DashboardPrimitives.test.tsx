import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";

import {
  DashboardPanel,
  DashboardPanelContent,
  DashboardPanelDescription,
  DashboardPanelFooter,
  DashboardPanelHeader,
  DashboardPanelTitle,
} from "./DashboardPanel";
import { DashboardRow, DashboardRowGroup } from "./DashboardRow";
import { MetaGrid } from "./MetaGrid";
import { StatusPill } from "./StatusPill";
import { TerminalPanel } from "./TerminalPanel";

describe("dashboard primitives", () => {
  it("renders a dashboard panel with header, content, and footer slots", () => {
    render(
      <DashboardPanel data-testid="panel">
        <DashboardPanelHeader>
          <div>
            <DashboardPanelTitle>Panel title</DashboardPanelTitle>
            <DashboardPanelDescription>Panel copy</DashboardPanelDescription>
          </div>
        </DashboardPanelHeader>
        <DashboardPanelContent>Panel body</DashboardPanelContent>
        <DashboardPanelFooter>Panel foot</DashboardPanelFooter>
      </DashboardPanel>,
    );

    expect(screen.getByTestId("panel")).toHaveAttribute("data-dashboard-panel", "");
    expect(screen.getByTestId("panel").className).toContain("bg-card");
    expect(screen.getByRole("heading", { level: 2, name: "Panel title" })).toBeTruthy();
    expect(screen.getByText("Panel copy")).toBeTruthy();
    expect(screen.getByText("Panel body")).toBeTruthy();
    expect(screen.getByText("Panel foot")).toBeTruthy();
  });

  it("renders operational rows as one bordered group", () => {
    render(
      <DashboardRowGroup data-testid="rows">
        <DashboardRow title="GitHub" description="Token required" action="Ready" />
      </DashboardRowGroup>,
    );

    expect(screen.getByTestId("rows").className).toContain("border-border");
    expect(screen.getByText("GitHub")).toBeTruthy();
    expect(screen.getByText("Token required")).toBeTruthy();
    expect(screen.getByText("Ready")).toBeTruthy();
  });

  it("renders a meta grid with tokenized labels", () => {
    render(<MetaGrid items={[{ label: "Credential", value: "GITHUB_TOKEN" }]} />);
    expect(screen.getByText("Credential").className).toContain("font-mono");
    expect(screen.getByText("GITHUB_TOKEN")).toBeTruthy();
  });

  it("renders status pills by variant", () => {
    render(<StatusPill variant="warning">Token required</StatusPill>);
    const pill = screen.getByText("Token required");
    expect(pill).toHaveAttribute("data-variant", "warning");
    expect(pill.className).toContain("text-warning");
  });

  it("renders a dashboard row with icon and meta, omitting description and action", () => {
    render(
      <DashboardRow icon={<span>ic</span>} title="OpenAI" meta={<span>meta-info</span>} />,
    );
    expect(screen.getByText("ic")).toBeTruthy(); // icon branch
    expect(screen.getByText("OpenAI")).toBeTruthy();
    expect(screen.getByText("meta-info")).toBeTruthy(); // meta branch
    // description + action omitted exercises their null branches
    expect(screen.queryByText("Token required")).toBeNull();
  });

  it("renders a status pill with a status dot", () => {
    render(
      <StatusPill variant="success" dot>
        Connected
      </StatusPill>,
    );
    const pill = screen.getByText("Connected");
    expect(pill).toHaveAttribute("data-variant", "success");
    expect(pill.querySelector("span.rounded-full")).toBeTruthy(); // dot branch
  });

  it("renders a bordered meta grid", () => {
    render(<MetaGrid bordered items={[{ label: "Account", value: "managed" }]} />);
    expect(screen.getByText("Account").closest("dl")?.className).toContain("border-t");
  });

  it("renders a dashboard panel as a child element via asChild (Slot)", () => {
    render(
      <DashboardPanel asChild>
        <section data-testid="as-child">panel as section</section>
      </DashboardPanel>,
    );
    const el = screen.getByTestId("as-child");
    expect(el.nodeName).toBe("SECTION");
    expect(el).toHaveAttribute("data-dashboard-panel", "");
  });

  it("renders terminal chrome without a tag", () => {
    render(<TerminalPanel title="vault">body only</TerminalPanel>);
    expect(screen.getByText("body only")).toBeTruthy();
    expect(screen.queryByText("write-only")).toBeNull(); // tag null branch
  });

  it("renders terminal chrome with title and tag", () => {
    render(
      <TerminalPanel title="vault" tag="write-only">
        terminal body
      </TerminalPanel>,
    );
    expect(screen.getByText("terminal body").parentElement).toHaveAttribute(
      "data-terminal-panel",
      "",
    );
    expect(screen.getByText("vault")).toBeTruthy();
    expect(screen.getByText("write-only")).toBeTruthy();
    expect(screen.getByText("terminal body")).toBeTruthy();
  });
});
