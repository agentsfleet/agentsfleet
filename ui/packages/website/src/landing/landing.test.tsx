import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { trackNavigationClicked, trackSignupStarted } from "../analytics/posthog";
import Home from "../pages/Home";
import { structuredData } from "./metadata";

vi.mock("../analytics/posthog", () => ({
  trackNavigationClicked: vi.fn(),
  trackSignupStarted: vi.fn(),
}));

const WAITLIST = "https://example.test/waitlist";

it("shows the first message and reflects approval in the fleet summary", () => {
  render(<Home waitlistUrl={WAITLIST} />);
  const demo = within(screen.getByRole("figure", { name: "Example fleet conversation" }));
  expect(demo.getByText("Tell me when a pull request merges in acme/web.")).toBeInTheDocument();
  expect(demo.getByRole("status")).toHaveTextContent("GitHub access requested");
  expect(demo.getByText("No access yet")).toBeInTheDocument();
  fireEvent.click(demo.getByRole("button", { name: "Approve" }));
  expect(demo.getByRole("status")).toHaveTextContent("GitHub access approved");
  expect(demo.getByText("GitHub · acme/web")).toBeInTheDocument();
  expect(demo.getByText("Pull request merges")).toBeInTheDocument();
  fireEvent.click(demo.getByRole("button", { name: "Reset example" }));
  expect(demo.getByRole("status")).toHaveTextContent("GitHub access requested");
  expect(demo.getByText("No triggers yet")).toBeInTheDocument();
});

it("keeps focus and leaves access empty after denying the request", () => {
  render(<Home waitlistUrl={WAITLIST} />);
  const demo = within(screen.getByRole("figure", { name: "Example fleet conversation" }));
  const deny = demo.getByRole("button", { name: "Deny" });
  deny.focus();
  fireEvent.click(deny);
  expect(demo.getByRole("status")).toHaveTextContent("GitHub access denied");
  expect(demo.getByText("No access yet")).toBeInTheDocument();
  expect(demo.getByRole("button", { name: "Reset example" })).toHaveFocus();
});

it("keeps waitlist links and attributes the next action", () => {
  render(<Home waitlistUrl={WAITLIST} />);
  for (const signup of screen.getAllByRole("link", { name: "Join waitlist" })) {
    expect(signup).toHaveAttribute("href", WAITLIST);
    expect(signup).toHaveAttribute("target", "_blank");
    expect(signup).toHaveAttribute("rel", "noopener noreferrer");
  }
  const hero = within(screen.getByRole("region", { name: "Start a fleet with one message." }));
  const examples = hero.getByRole("link", { name: "See examples" });
  expect(examples).toHaveAttribute("href", "/examples");
  fireEvent.click(examples);
  fireEvent.click(hero.getByRole("link", { name: "Join waitlist" }));
  expect(trackSignupStarted).toHaveBeenCalledWith({ source: "hero_signup", surface: "hero", mode: "humans" });
  expect(trackNavigationClicked).toHaveBeenCalledWith({ source: "hero_examples", surface: "hero", target: "examples" });
});

it("publishes the same FAQ answers in the page and its search metadata", () => {
  const { container } = render(<Home waitlistUrl={WAITLIST} />);
  const questions = Array.from(container.querySelectorAll("#faq article"), (article) => ({
    "@type": "Question",
    name: article.querySelector("h3")?.textContent,
    acceptedAnswer: { "@type": "Answer", text: article.querySelector("p")?.textContent },
  }));
  const faqSchema = structuredData["@graph"].find((entry) => entry["@type"] === "FAQPage");
  expect(questions.length).toBeGreaterThan(0);
  expect(faqSchema?.mainEntity).toEqual(questions);
});
