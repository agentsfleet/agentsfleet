import {afterEach, expect, it, vi} from "vitest";
import {cleanup, render, screen} from "@testing-library/react";
import {ResourceHeader} from "./ResourceHeader";

const route = vi.hoisted(() => ({pathname: "/settings/members"}));
vi.mock("next/navigation", () => ({usePathname: () => route.pathname}));
vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => ({userId: "viewer"})}));
afterEach(cleanup);

it.each([
  ["/settings/members", "Team"], ["/w/workspace/settings/models", "Models"],
  ["/settings/billing", "Usage"], ["/w/workspace/library", "Plugins"],
])("labels the current resource at %s", (pathname, label) => {
  route.pathname = pathname;
  render(<ResourceHeader />);
  expect(screen.getByText(label)).toBeTruthy();
});

it.each(["/w/workspace/fleets/selected", "/w/workspace/fleets/new", "/unknown"])("leaves the existing conversation or unknown chrome at %s", pathname => {
  route.pathname = pathname;
  const {container} = render(<ResourceHeader />);
  expect(container.textContent).toBe("");
});
