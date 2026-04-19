import { describe, it, expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { BackgroundBeamsWithCollision } from "./background-beams-with-collision";

/*
 * SSR contract for BackgroundBeamsWithCollision (§5.8.3 dim 5.8.1).
 * The parent shell + floor ref render during SSR; the motion beams
 * themselves rely on `window` / IntersectionObserver and only appear
 * post-mount. renderToStaticMarkup proves the server pass is safe.
 */

describe("BackgroundBeamsWithCollision — SSR", () => {
  it("renders the decorative shell marked presentation", () => {
    const html = renderToStaticMarkup(
      <BackgroundBeamsWithCollision>
        <h2>Hello</h2>
      </BackgroundBeamsWithCollision>,
    );
    expect(html).toContain('role="presentation"');
    expect(html).toContain("<h2>Hello</h2>");
  });

  it("uses the brand gradient background (not neutral-100)", () => {
    const html = renderToStaticMarkup(<BackgroundBeamsWithCollision />);
    expect(html).toContain("bg-gradient-to-b");
    expect(html).toContain("from-background");
  });
});
