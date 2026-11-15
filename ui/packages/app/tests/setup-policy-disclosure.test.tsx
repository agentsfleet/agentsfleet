import {cleanup, render, screen, waitFor} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {afterEach, describe, expect, it, vi} from "vitest";
import {TooltipProvider} from "@agentsfleet/design-system";
import {SetupSummary} from "@/components/domain/setup/summary/SetupSummary";
import {REVIEW} from "@/tests/helpers/draft-fixtures";

afterEach(cleanup);

describe("setup policy explanation", () => {
  it("opens help and returns focus without changing the policy", async () => {
    const user = userEvent.setup();
    const change = vi.fn();
    render(<TooltipProvider><SetupSummary requirements={REVIEW.requirements} onNetworkChange={change} /></TooltipProvider>);
    const help = screen.getByRole("button", {name: "About package manager domains"});
    help.focus(); await user.click(help);
    const dialog = await screen.findByRole("dialog", {name: "Package manager domains"});
    expect(dialog.textContent).toContain("Allow standard package registries while public internet access is restricted.");
    expect(dialog.textContent).toContain("Its selection is kept when Allow all is switched off.");
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(help));
    await user.keyboard("{Enter}");
    await screen.findByRole("dialog", {name: "Package manager domains"});
    await user.click(screen.getByRole("button", {name: "Done"}));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(screen.getByRole("switch", {name: "Allow all public internet access"}).getAttribute("aria-checked")).toBe("false");
    expect(screen.getByRole("switch", {name: "Enable package manager domains"}).getAttribute("aria-checked")).toBe("false");
    expect(change).not.toHaveBeenCalled();
  });

  it("keeps policy help available when setup changes are disabled", async () => {
    const user = userEvent.setup();
    const change = vi.fn();
    render(<TooltipProvider><SetupSummary requirements={REVIEW.requirements} disabled onNetworkChange={change} /></TooltipProvider>);
    expect(screen.getByRole("switch", {name: "Allow all public internet access"})).toHaveProperty("disabled", true);
    expect(screen.getByRole("switch", {name: "Enable package manager domains"})).toHaveProperty("disabled", true);
    const help = screen.getByRole("button", {name: "About package manager domains"});
    expect(help).toHaveProperty("disabled", false); await user.click(help);
    expect((await screen.findByRole("dialog", {name: "Package manager domains"})).textContent).toContain("This option is disabled when Allow all is on.");
    expect(change).not.toHaveBeenCalled();
  });

  it("does not offer a policy explanation before network policy has been reviewed", () => {
    render(<TooltipProvider><SetupSummary ready /></TooltipProvider>);
    expect(screen.getByText("Network policy not reviewed")).not.toBeNull();
    expect(screen.queryByRole("button", {name: "About package manager domains"})).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("keeps the selected package policy while public internet disables its switch", async () => {
    const user = userEvent.setup(), change = vi.fn();
    render(<TooltipProvider><SetupSummary requirements={{...REVIEW.requirements,
      network: {http_egress: "public", package_manager_domains: true, allow: []}}} onNetworkChange={change} /></TooltipProvider>);
    const packages = screen.getByRole("switch", {name: "Enable package manager domains"});
    expect(packages).toHaveProperty("disabled", true);
    expect(packages.getAttribute("aria-checked")).toBe("true");
    await user.click(screen.getByRole("button", {name: "About package manager domains"}));
    await screen.findByRole("dialog", {name: "Package manager domains"});
    await user.click(screen.getByRole("button", {name: "Done"}));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(packages.getAttribute("aria-checked")).toBe("true");
    expect(change).not.toHaveBeenCalled();
  });
});
