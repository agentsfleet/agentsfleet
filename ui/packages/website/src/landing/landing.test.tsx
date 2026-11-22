import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import Home from "../pages/Home";
import { structuredData } from "./metadata";
import { DEMO_STAGES } from "./useDemoPlayback";
import {
  trackNavigationClicked,
  trackSignupStarted,
} from "../analytics/posthog";

vi.mock("../analytics/posthog", () => ({
  initAnalytics: vi.fn(),
  trackNavigationClicked: vi.fn(),
  trackSignupStarted: vi.fn(),
}));

const reduce = Object.assign(new EventTarget(), { matches: false });

beforeEach(() => {
  reduce.matches = false;
  vi.stubGlobal("matchMedia", () => reduce);
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

it("advances in view, holds the scene during inspection, and replays after the last reply", () => {
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
  const demo = screen.getByRole("figure", { name: "Why did chat freeze?" });
  const stage = demo.querySelector("[data-playing]")!;
  const control = within(demo).getByRole("button", { name: "Pause demo" });
  const advance = (ms: number) => act(() => vi.advanceTimersByTime(ms));
  const visible = (value: boolean) =>
    act(() =>
      intersect([{ isIntersecting: value, intersectionRatio: Number(value) }]),
    );

  expect(stage).toHaveAttribute("data-playing", "false");
  visible(true);
  expect(stage).toHaveAttribute("data-playing", "true");
  advance(1000);
  visible(false);
  advance(10000);
  expect(screen.getByText("Slack ticket", { exact: true })).toBeInTheDocument();
  visible(true);
  advance(DEMO_STAGES[0].duration - 1000);
  expect(
    screen.getByText("Grafana investigation", { exact: true }),
  ).toBeInTheDocument();

  fireEvent.click(screen.getByText("Worked for 4s · 2 tools"));
  expect(control).toHaveTextContent("Resume demo");
  advance(DEMO_STAGES[1].duration * 2);
  expect(
    screen.getByText("Grafana investigation", { exact: true }),
  ).toBeInTheDocument();
  fireEvent.click(control);
  advance(DEMO_STAGES[1].duration);
  expect(screen.getByText("Draft fix", { exact: true })).toBeInTheDocument();
  advance(DEMO_STAGES[2].duration);
  expect(
    screen.getByText("Verified result", { exact: true }),
  ).toBeInTheDocument();
  advance(DEMO_STAGES[3].duration);
  expect(control).toHaveTextContent("Replay demo");

  fireEvent.click(control);
  expect(screen.getByText("Slack ticket", { exact: true })).toBeInTheDocument();
  reduce.matches = true;
  act(() => reduce.dispatchEvent(new Event("change")));
  expect(stage).toHaveAttribute("data-playing", "false");
  expect(stage.querySelectorAll("[inert]")).toHaveLength(0);
});

it("works without an intersection observer and records signup and navigation clicks", () => {
  Reflect.deleteProperty(window, "IntersectionObserver");
  render(<Home waitlistUrl="https://example.test/waitlist" />);
  expect(
    document.querySelector("#product-demo [data-playing]"),
  ).toHaveAttribute("data-playing", "true");
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
  const { container } = render(
    <Home waitlistUrl="https://example.test/waitlist" />,
  );
  const questions = Array.from(
    container.querySelectorAll("#faq article"),
    (article) => ({
      "@type": "Question",
      name: article.querySelector("h3")?.textContent,
      acceptedAnswer: {
        "@type": "Answer",
        text: article.querySelector("p")?.textContent,
      },
    }),
  );
  const faqSchema = structuredData["@graph"].find(
    (entry) => entry["@type"] === "FAQPage",
  );
  expect(questions.length).toBeGreaterThan(0);
  expect(faqSchema?.mainEntity).toEqual(questions);
});
