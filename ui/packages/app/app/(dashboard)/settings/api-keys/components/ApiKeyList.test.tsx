import { readFileSync } from "node:fs";
import { join } from "node:path";
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";
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
const LAST_USED_AT = Date.UTC(2026, 0, 3, 6, 7, 8);

function makeResponse(row: Partial<ApiKeyRow> = {}): ApiKeyListResponse {
  const item: ApiKeyRow = {
    id: "key_1",
    key_name: "ci-token",
    active: true,
    created_at: CREATED_AT,
    last_used_at: LAST_USED_AT,
    revoked_at: null,
    ...row,
  };
  return { items: [item], total: 1, page: 1, page_size: 25 };
}

function renderList(response: ApiKeyListResponse) {
  return render(
    React.createElement(
      TooltipProvider,
      null,
      React.createElement(ApiKeyList, { initial: response }),
    ),
  );
}

afterEach(() => cleanup());

describe("ApiKeyList timestamps", () => {
  it("test_apikeylist_uses_time_no_fmt", () => {
    const { container } = renderList(makeResponse());

    // Row renders <time> elements for created + last-used, each carrying the
    // canonical ISO datetime attribute from the epoch-ms field.
    const times = Array.from(container.querySelectorAll("time"));
    const isos = times.map((t) => t.getAttribute("dateTime"));
    expect(isos).toContain(new Date(CREATED_AT).toISOString());
    expect(isos).toContain(new Date(LAST_USED_AT).toISOString());

    // Source is free of the bare, unpinned formatter it replaced.
    const src = readFileSync(join(__dirname, "ApiKeyList.tsx"), "utf8");
    expect(src).not.toContain("function fmt(");
    expect(src).not.toContain("toLocaleString");
  });

  it("renders no Time for a null last_used_at (guard preserved)", () => {
    const { container } = renderList(makeResponse({ last_used_at: null }));
    const isos = Array.from(container.querySelectorAll("time")).map((t) =>
      t.getAttribute("dateTime"),
    );
    // Only the created timestamp is present; the never-used branch stays text.
    expect(isos).toEqual([new Date(CREATED_AT).toISOString()]);
    expect(container.textContent).toContain("never used");
  });
});
