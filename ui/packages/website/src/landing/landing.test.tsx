import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { render } from "@testing-library/react";
import Home from "../pages/Home";
import { structuredData } from "./metadata";
import {
  trackNavigationClicked,
  trackSignupStarted,
} from "../analytics/posthog";

vi.mock("../analytics/posthog", () => ({
  initAnalytics: vi.fn(),
  trackNavigationClicked: vi.fn(),
  trackSignupStarted: vi.fn(),
}));

function mediaQuery(matches: boolean) {
  return Object.assign(new EventTarget(), { matches });
}

const reduce = mediaQuery(false);
const small = mediaQuery(false);

beforeEach(() => {
  reduce.matches = false;
  small.matches = false;
  vi.stubGlobal("matchMedia", (query: string) =>
    query.includes("reduced-motion") ? reduce : small,
  );
});

afterEach(() => {
  vi.unstubAllGlobals();
});

it("plays in view, honors pause and motion preferences, and lets the reader inspect the evidence", () => {
  let intersect!: (entries: Partial<IntersectionObserverEntry>[]) => void;
  vi.stubGlobal(
    "IntersectionObserver",
    class {
      constructor(callback: typeof intersect) {
        intersect = callback;
      }
      observe() {}
      disconnect() {}
    },
  );
  render(<Home waitlistUrl="https://example.test/waitlist" />);
  const demo = document.querySelector(".film")!;
  const control = demo.querySelector("button")!;
  const summary = demo.querySelector<HTMLElement>(".setup summary")!;
  expect(demo).not.toHaveAttribute("data-playing");
  intersect([{ isIntersecting: false, intersectionRatio: 0 }]);
  intersect([{ isIntersecting: true, intersectionRatio: 0.2 }]);
  expect(demo).not.toHaveAttribute("data-playing");
  intersect([{ isIntersecting: true, intersectionRatio: 0.8 }]);
  expect(demo).toHaveAttribute("data-playing");
  control.click();
  expect(control).toHaveTextContent("Resume demo");
  control.click();
  expect(demo).toHaveAttribute("data-playing");
  summary.click();
  expect(demo).not.toHaveAttribute("data-playing");
  control.click();
  demo.querySelector(".meter span")!.dispatchEvent(new Event("animationend"));
  expect(control).toHaveTextContent("Replay demo");
  summary.click();
  expect(control).toHaveTextContent("Replay demo");
  control.click();
  expect(demo).toHaveAttribute("data-playing");
  small.matches = true;
  small.dispatchEvent(new Event("change"));
  expect(summary.parentElement).not.toHaveAttribute("open");
  reduce.matches = true;
  reduce.dispatchEvent(new Event("change"));
  summary.click();
  expect(demo).not.toHaveAttribute("data-motion");
  expect(control.parentElement).toHaveAttribute("hidden");
});

it("works without an intersection observer and records signup and navigation clicks", () => {
  Reflect.deleteProperty(window, "IntersectionObserver");
  render(<Home waitlistUrl="https://example.test/waitlist" />);
  expect(document.querySelector(".film")).toHaveAttribute("data-playing");
  for (const source of ["hero_signup", "hero_github"]) {
    const link = document.querySelector<HTMLAnchorElement>(
      `a[data-source="${source}"]`,
    )!;
    link.addEventListener("click", (event) => event.preventDefault());
    link.click();
  }
  expect(trackSignupStarted).toHaveBeenCalledWith({
    source: "hero_signup",
    surface: "hero",
    mode: "humans",
  });
  expect(trackNavigationClicked).toHaveBeenCalledWith({
    source: "hero_github",
    surface: "hero",
    target: "github",
  });
});

it("publishes the same FAQ answers in the page and its search metadata", () => {
  const { container } = render(<Home waitlistUrl="https://example.test/waitlist" />);
  const questions = Array.from(container.querySelectorAll("#faq article"), (article) => ({
    "@type": "Question",
    name: article.querySelector("h3")?.textContent,
    acceptedAnswer: { "@type": "Answer", text: article.querySelector("p")?.textContent },
  }));
  const faqSchema = structuredData["@graph"].find((entry) => entry["@type"] === "FAQPage");
  expect(questions.length).toBeGreaterThan(0);
  expect(faqSchema?.mainEntity).toEqual(questions);
});
