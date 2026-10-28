import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { connectorReturnPath, connectorFailureCopy } from "@/lib/connectors/completion";
import CompletionPage from "../app/connectors/complete/page";

afterEach(cleanup);

describe("connector recovery hints", () => {
  it.each(["//evil.example", "https://evil.example", "/w/../integrations", "/w/ws/integrations?code=private", "/w/ws/integrations#fragment", undefined])("refuses an unsafe return path: %s", value => {
    expect(connectorReturnPath(value)).toBeNull();
  });
  it("retains only a workspace integrations path", () => {
    expect(connectorReturnPath("/w/ws_1/integrations")).toBe("/w/ws_1/integrations");
  });
  it("never echoes an unknown reason", () => {
    expect(connectorFailureCopy("private-vendor-detail")).toBe(connectorFailureCopy("unavailable"));
    expect(connectorFailureCopy("constructor")).toBe(connectorFailureCopy("unavailable"));
  });

  it("offers a fresh workspace attempt without exposing provider details", async () => {
    render(await CompletionPage({ searchParams: Promise.resolve({
      reason: "private-vendor-detail", return_to: "/w/ws_1/integrations",
    }) }));
    expect(screen.getByRole("link", { name: "Back to Connect apps" }).getAttribute("href"))
      .toBe("/w/ws_1/integrations");
    expect(screen.queryByText("private-vendor-detail")).toBeNull();
  });

  it.each([undefined, "https://evil.example"])("falls back to the dashboard for %s", async return_to => {
    render(await CompletionPage({ searchParams: Promise.resolve({ return_to }) }));
    expect(screen.getByRole("link", { name: "Back to dashboard" }).getAttribute("href"))
      .toBe("/");
  });
});
