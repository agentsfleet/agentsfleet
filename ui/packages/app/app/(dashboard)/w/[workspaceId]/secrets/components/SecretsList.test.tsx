import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { TooltipProvider, formatTimeAbsolute } from "@agentsfleet/design-system";
import { SECRET_KIND, type Secret } from "@/lib/api/secrets";

// next/navigation + the server action module are the only runtime deps
// SecretsList reaches for; the dynamic edit/rename islands render null while
// closed (open is parent-driven), so no stub is needed for them.
vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: vi.fn(), push: vi.fn() }),
}));
vi.mock("../actions", () => ({ deleteSecretAction: vi.fn() }));

import SecretsList from "./SecretsList";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SOURCE_PATH = path.join(__dirname, "SecretsList.tsx");

// A fixed, far-past instant keeps the relative label stable ("… ago", never
// flipping to "in …") and the absolute tooltip deterministic per runtime tz.
const CREATED_MS = Date.UTC(2020, 0, 15, 10, 30, 0);

function providerSecret(created_at: number): Secret {
  return { kind: SECRET_KIND.provider_key, name: "openai", provider: "openai", created_at };
}

function renderList(secrets: Secret[]) {
  return render(
    React.createElement(
      TooltipProvider,
      null,
      React.createElement(SecretsList, { workspaceId: "ws_1", secrets }),
    ),
  );
}

afterEach(() => cleanup());

describe("SecretsList Created cell", () => {
  it("test_secrets_created_relative", async () => {
    const { container } = renderList([providerSecret(CREATED_MS)]);

    // The Created cell renders a <time> whose datetime is the ISO instant and
    // whose visible text is the relative "… ago" label.
    const timeEl = container.querySelector("time");
    expect(timeEl).not.toBeNull();
    expect(timeEl!.getAttribute("datetime")).toBe(new Date(CREATED_MS).toISOString());
    expect(timeEl!.textContent).toMatch(/ago$/);

    // Focus opens the Radix tooltip, which carries the absolute timestamp.
    const absolute = formatTimeAbsolute(new Date(CREATED_MS));
    fireEvent.focus(timeEl!);
    const tips = await screen.findAllByText(absolute);
    expect(tips.length).toBeGreaterThan(0);
  });

  it("test_secretslist_no_bespoke_formatter", () => {
    const source = readFileSync(SOURCE_PATH, "utf8");
    expect(source).not.toContain("DATE_FORMATTER");
    expect(source).not.toContain("formatCreatedAt");
  });
});
