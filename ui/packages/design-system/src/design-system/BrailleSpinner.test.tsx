import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";

import { BRAILLE_FRAMES, BRAILLE_REST_FRAME, BrailleSpinner } from "./BrailleSpinner";

const SOURCE = readFileSync(path.join(__dirname, "BrailleSpinner.tsx"), "utf8");

describe("BrailleSpinner", () => {
  it("test_braille_spinner_frames_and_animation", () => {
    const { container } = render(<BrailleSpinner className="text-pulse" />);
    const root = container.firstElementChild as HTMLElement;
    expect(root.getAttribute("aria-hidden")).toBe("true");
    expect(root.hasAttribute("data-braille-spinner")).toBe(true);
    expect(root.className).toBe("text-pulse");
    const column = root.querySelector("[data-braille-frames]");
    // Pin test: the frames and their order are the animation.
    expect([...(column?.children ?? [])].map((frame) => frame.textContent)).toEqual([
      "⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏",
    ]);
    expect(BRAILLE_FRAMES).toHaveLength(10);
    // The reduced-motion stand-in rides beside the column; tokens.css swaps them.
    expect(root.querySelector("[data-braille-rest]")?.textContent).toBe(BRAILLE_REST_FRAME);
    // No per-frame script: the primitive holds no state, effect or timer.
    expect(SOURCE).not.toMatch(/\buse[A-Z]\w*\(|setInterval|setTimeout|requestAnimationFrame/);
  });
});
