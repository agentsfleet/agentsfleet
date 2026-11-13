import {type ComponentType, type PropsWithChildren, useEffect, useState} from "react";
import {cleanup, render, screen} from "@testing-library/react";
import {afterEach, expect, it, vi} from "vitest";

vi.mock("@/components/domain/FleetMarkdown", () => ({FleetMarkdown: ({children}: PropsWithChildren) => <article>{children}</article>}));
vi.mock("next/dynamic", () => ({default: (
  load: () => Promise<ComponentType<PropsWithChildren>>, options: {loading: () => React.ReactNode},
) => function Instructions(props: PropsWithChildren) {
  const [View, setView] = useState<ComponentType<PropsWithChildren> | null>(null);
  useEffect(() => {void load().then(component => setView(() => component));}, []);
  return View ? <View {...props} /> : options.loading();
}}));
import {FleetMarkdownDynamic} from "@/components/domain/FleetMarkdownDynamic";
afterEach(cleanup);

// Value: protects=lazy instructions availability; fails_when=the named renderer export or loading announcement is lost;
// seam=Next dynamic loader; no_claim=Markdown parsing, which has its own suite.
it("announces loading and forwards instructions to the lazily loaded renderer", async () => {
  render(<FleetMarkdownDynamic>Check the deployment before publishing.</FleetMarkdownDynamic>);
  expect(screen.getByLabelText("Loading instructions")).toBeTruthy();
  expect(await screen.findByText("Check the deployment before publishing.")).toBeTruthy();
  expect(screen.queryByLabelText("Loading instructions")).toBeNull();
});
