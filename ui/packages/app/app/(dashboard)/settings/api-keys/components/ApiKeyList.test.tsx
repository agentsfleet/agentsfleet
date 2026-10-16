import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";

// The list's server-action module pulls the network client (server-only). The
// render tests below never invoke an action, so a shallow mock keeps the module
// graph client-safe.
vi.mock("../actions", () => ({
  listApiKeysAction: vi.fn(),
  revokeApiKeyAction: vi.fn(),
  deleteApiKeyAction: vi.fn(),
}));

import ApiKeyList from "./ApiKeyList";
import type { ApiKeyListResponse, ApiKeyRow } from "@/lib/api/api_keys";

const CREATED_AT = Date.UTC(2026, 0, 2, 3, 4, 5); // 2026-01-02T03:04:05.000Z

function makeResponse(row: Partial<ApiKeyRow> = {}): ApiKeyListResponse {
  const item: ApiKeyRow = {
    id: "key_1",
    key_name: "ci-token",
    active: true,
    created_at: CREATED_AT,
    revoked_at: null,
    ...row,
  };
  return { items: [item], total: 1, next_cursor: null };
}

// The wrapper stands in for a real ancestor: the root layout mounts the app's
// one TooltipProvider, which the relative `Time` cells here read from.
function renderList(response: ApiKeyListResponse) {
  return render(
    React.createElement(ApiKeyList, { initial: response }),
    { wrapper: TooltipProvider },
  );
}

afterEach(() => cleanup());

describe("ApiKeyList timestamps", () => {
  it("shows only the available created and revoked instants", () => {
    const revoked = Date.UTC(2026, 0, 3, 6, 7, 8);
    const {container} = renderList(makeResponse({active: false, revoked_at: revoked}));
    const times = Array.from(container.querySelectorAll("time")).map((node) => node.getAttribute("dateTime"));
    expect(times).toContain(new Date(CREATED_AT).toISOString());
    expect(times).toContain(new Date(revoked).toISOString());
    expect(container.textContent).not.toMatch(/last used|never used/i);
  });
  it("shows created time alone for an active key", () => {
    const {container} = renderList(makeResponse());
    expect(Array.from(container.querySelectorAll("time")).map((node) => node.getAttribute("dateTime")))
      .toEqual([new Date(CREATED_AT).toISOString(), new Date(CREATED_AT).toISOString()]);
    expect(container.textContent).not.toMatch(/last used|never used/i);
  });
});

it("filters and restores loaded API keys without a server reload", async () => {
  renderList(makeResponse());
  fireEvent.change(screen.getByRole("searchbox", {name: "Search API keys"}), {target: {value: "missing"}});
  await screen.findByText("No matching keys");
  expect(screen.queryByRole("button", {name: "Revoke API key ci-token"})).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Clear search"}));
  await waitFor(() => expect(screen.getByRole("button", {name: "Revoke API key ci-token"})).toBeTruthy());
});
