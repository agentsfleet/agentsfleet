import { afterEach, describe, expect, it, vi } from "vitest";
import { MintUnavailable, mintOperationId } from "./operation-id";

// The shape the daemon stores as `producer_key`, and the two RFC 9562 marks a
// v7 carries: version nibble `7`, variant in `[89ab]`.
const UUID_V7 = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
// 2026-09-28T00:00:00Z, and its 48-bit big-endian spelling as the first
// twelve hex digits of the id.
const INSTANT_MS = 1_790_553_600_000;
const INSTANT_HEX = "01a0e54fb000";
const TIMESTAMP_HEX_DIGITS = INSTANT_HEX.length;
const MINTS = 50;

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

function timestampHex(id: string): string {
  return id.replaceAll("-", "").slice(0, TIMESTAMP_HEX_DIGITS);
}

describe("mintOperationId", () => {
  it("test_mint_sorts_by_time_then_refuses", () => {
    vi.useFakeTimers({ now: INSTANT_MS, toFake: ["Date"] });
    const first = mintOperationId();
    expect(first).toMatch(UUID_V7);
    expect(timestampHex(first)).toBe(INSTANT_HEX);

    // Strictly increasing as text within one document, even inside one
    // millisecond — the ledger lists sends in the order they were made.
    const minted = Array.from({ length: MINTS }, () => mintOperationId());
    expect(new Set(minted).size).toBe(MINTS);
    expect([...minted].sort()).toEqual(minted);
    vi.setSystemTime(INSTANT_MS + 1);
    expect(mintOperationId() > (minted.at(-1) ?? "")).toBe(true);

    // No generator: the send cannot be named, so it is refused, never sent
    // without an identity. The platform's own failure rides as the cause.
    vi.stubGlobal("crypto", {});
    expect(() => mintOperationId()).toThrow(MintUnavailable);
    vi.stubGlobal("crypto", undefined);
    const refused = (() => {
      try {
        mintOperationId();
      } catch (error) {
        return error;
      }
      return null;
    })();
    expect(refused).toBeInstanceOf(MintUnavailable);
    expect((refused as MintUnavailable).cause).toBeInstanceOf(Error);
  });
});
