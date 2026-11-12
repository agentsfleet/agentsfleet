import {StrictMode} from "react";
import {cleanup, render} from "@testing-library/react";
import {afterEach, beforeEach, expect, it, vi} from "vitest";
import {AuthoringIntentContext, AuthoringPrompt} from "@/components/domain/chat/AuthoringIntent";

const composer = vi.hoisted(() => ({text: "", setText: vi.fn()}));
vi.mock("@assistant-ui/react", () => ({useAui: () => ({composer: () => ({getState: () => ({text: composer.text}), setText: composer.setText})})}));
beforeEach(() => {composer.text = ""; composer.setText.mockReset();});
afterEach(cleanup);

// Value: protects=user-authored composer text; fails_when=starter navigation replaces a draft;
// seam=native composer API; no_claim=message transport.
it("prefills an empty new chat once, including a strict-mode effect replay", () => {
  const view = render(<StrictMode><AuthoringIntentContext value={{prompt: "Create a plugin"}}><AuthoringPrompt hasMessages={false} /></AuthoringIntentContext></StrictMode>);
  expect(composer.setText).toHaveBeenCalledExactlyOnceWith("Create a plugin");
  view.rerender(<StrictMode><AuthoringIntentContext value={{prompt: "Different starter"}}><AuthoringPrompt hasMessages={false} /></AuthoringIntentContext></StrictMode>);
  expect(composer.setText).toHaveBeenCalledOnce();
});

it.each([{hasMessages: true, text: ""}, {hasMessages: false, text: "My unsent draft"}])("preserves an existing conversation or composer draft: %o", ({hasMessages, text}) => {
  composer.text = text;
  render(<AuthoringIntentContext value={{prompt: "Create a plugin"}}><AuthoringPrompt hasMessages={hasMessages} /></AuthoringIntentContext>);
  expect(composer.setText).not.toHaveBeenCalled();
});

it("leaves an ordinary new chat empty", () => {
  render(<AuthoringPrompt hasMessages={false} />);
  expect(composer.setText).not.toHaveBeenCalled();
});
