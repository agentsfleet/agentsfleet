/**
 * Unit lane for the shared paint-boundary blank-frame audit. Its source lives
 * in the e2e fixtures tree (the journeys evaluate it in the real browser);
 * vitest excludes tests/e2e/** as test FILES, so the sibling unit test drives
 * the same logic here against happy-dom's frame loop.
 */
import { afterEach, beforeEach, expect, it } from "vitest";
import {
  installPaintBoundaryAudit,
  readBlankFrames,
  type ShellBlankAudit,
} from "./e2e/acceptance/fixtures/blank-frame-audit";

type AuditedWindow = typeof window & { __shellBlankAudit?: ShellBlankAudit };

function nextFrame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()));
}

beforeEach(() => {
  document.body.innerHTML = "<main>alive</main>";
});

afterEach(() => {
  const audited = window as AuditedWindow;
  if (audited.__shellBlankAudit) {
    audited.__shellBlankAudit.stopped = true;
    delete audited.__shellBlankAudit;
  }
  document.body.innerHTML = "";
});

it("test_paint_boundary_audit_detects_real_blank", async () => {
  installPaintBoundaryAudit();
  await nextFrame();

  const main = document.querySelector("main");
  main!.textContent = "";
  await nextFrame();
  main!.textContent = "restored";
  await nextFrame();

  expect(readBlankFrames()).toBe(1);
});

it("does not count a blank that never crosses a frame boundary", async () => {
  installPaintBoundaryAudit();
  await nextFrame();

  const main = document.querySelector("main");
  // Emptied and refilled within one task: no frame ever paints this state,
  // which is exactly the mutation-time false positive the paint-boundary
  // sampling exists to eliminate.
  main!.textContent = "";
  main!.textContent = "restored";
  await nextFrame();
  await nextFrame();

  expect(readBlankFrames()).toBe(0);
});

it("keeps refusing a replaced main region", async () => {
  installPaintBoundaryAudit();
  await nextFrame();
  document.body.innerHTML = "<main>impostor</main>";
  expect(() => readBlankFrames()).toThrow(/main region was replaced/);
});
