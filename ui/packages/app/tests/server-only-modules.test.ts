import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { findServerOnlyModule, findStaleMarker } from "../scripts/server-only-modules.mjs";

// The guard `.size-limit.mjs` runs after a build. Its decisions are pure over
// the sources it reads, so they are proved here without one: a chunk carrying
// the marker is a leak, a runtime without the marker is a stale guard.

const EFFECT = { name: "effect", marker: "effect/Effect/Yield", source: "node_modules/effect/dist/internal/core.js" };
const MARKERS = [EFFECT];
const CLEAN_CHUNK = "export const a = 1;";

describe("the server-only module guard", () => {
  it("the installed Effect runtime carries the marker the guard looks for", () => {
    const runtime = readFileSync(path.resolve(__dirname, "..", EFFECT.source), "utf8");
    expect(findStaleMarker(new Map([[EFFECT.name, runtime]]), MARKERS)).toBeNull();
  });

  it("a runtime that lost the marker, or is absent, makes the guard red", () => {
    expect(findStaleMarker(new Map([[EFFECT.name, CLEAN_CHUNK]]), MARKERS)).toEqual({ name: EFFECT.name, marker: EFFECT.marker });
    expect(findStaleMarker(new Map(), MARKERS)).toEqual({ name: EFFECT.name, marker: EFFECT.marker });
  });

  it("a client chunk carrying the marker is named; clean chunks pass", () => {
    const chunks = new Map([
      [".next/static/chunks/a.js", CLEAN_CHUNK],
      [".next/static/chunks/b.js", `Symbol.for("${EFFECT.marker}")`],
    ]);
    expect(findServerOnlyModule(chunks, MARKERS)).toEqual({ file: ".next/static/chunks/b.js", name: EFFECT.name });
    expect(findServerOnlyModule(new Map([[".next/static/chunks/a.js", CLEAN_CHUNK]]), MARKERS)).toBeNull();
  });
});
