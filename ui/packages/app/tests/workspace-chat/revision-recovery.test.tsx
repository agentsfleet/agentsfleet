import type { Message } from "@/lib/api/draft-types";
import { afterEach, beforeEach, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { CHAT_PROPS, chatRouter, chooseSetupAction } from "./fixture";
import { WorkspaceChat } from "@/components/domain/chat/WorkspaceChat";
import { DRAFT_EVENT } from "@/lib/api/draft-events";
import { MESSAGE_ROLE, MESSAGE_STATE } from "@/lib/api/draft-types";
import { SOURCE_KIND } from "@/lib/api/draft-vocabulary";
import { DraftClientFixture } from "@/tests/helpers/draft-client-fixture";
import { DRAFT_ID, FLEET_DETAIL, FLEET_ID, MESSAGE, REVIEW, response } from "@/tests/helpers/draft-fixtures";
import { FakeEventSource } from "@/tests/helpers/fake-event-source";

let fixture: DraftClientFixture;
beforeEach(() => {
  fixture = new DraftClientFixture(); fixture.mount(); chatRouter.replace.mockClear();
  fixture.fleet = {...FLEET_DETAIL, status: "active", setup: {state: "ready", checks: []}};
  fixture.current = {...fixture.current, linked_fleet_id: FLEET_ID,
    review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
});
afterEach(() => {cleanup(); fixture.dispose();});

// Value: protects=revision navigation during an accepted turn; fails_when=an effect resets a busy chat;
// seam=HTTP and event stream boundaries; no_claim=browser navigation or server completion.
it("defers a new source revision until the accepted turn completes", async () => {
  let messages: Message[] = [{...MESSAGE, state: MESSAGE_STATE.running}];
  let contextReads = 0;
  fixture.override = ({path}) => {
    if (path.endsWith("/authoring-context")) {
      contextReads++;
      return response({fleet: fixture.fleet, conversations: [{draft: fixture.current, messages,
        messages_starting_after: null, truncated: false}], activities: {items: [], next_cursor: null}, next_cursor: null});
    }
    if (path.endsWith("/messages") && path.includes("/drafts/")) return response({items: messages, next_cursor: null});
  };
  const source = {kind: SOURCE_KIND.fleet, fleet_id: FLEET_ID, setup_revision: "1"};
  const page = render(<WorkspaceChat {...CHAT_PROPS} source={source} />);
  await screen.findByRole("status", {name: "Working"});
  await waitFor(() => expect(FakeEventSource.instances.at(-1)).toBeTruthy());
  fixture.fleet = {...fixture.fleet, setup_revision: "2", name: "Updated incident watch"};
  page.rerender(<WorkspaceChat {...CHAT_PROPS} source={{...source, setup_revision: "2"}} />);
  expect(screen.getByRole("status", {name: "Working"})).toBeTruthy();
  expect(contextReads).toBe(1);
  expect(fixture.mutations()).toEqual([]);
  messages = [MESSAGE, {...MESSAGE, role: MESSAGE_ROLE.assistant, text: "The accepted turn finished."}];
  await act(async () => FakeEventSource.instances.at(-1)!.emitNamed(DRAFT_EVENT.completed, JSON.stringify({draft_id: DRAFT_ID})));
  await screen.findByRole("heading", {name: fixture.fleet.name});
  expect(screen.getByText("The accepted turn finished.")).toBeTruthy();
  expect(contextReads).toBe(2);
  expect(fixture.mutations()).toEqual([]);
});

// Value: protects=the fleet-page chat stays at its owning fleet after a manual save;
// fails_when=the shared authoring URL effect navigates to the new-fleet page; seam=HTTP and router.
it("keeps a saved fleet-page conversation on the fleet page", async () => {
  fixture.override = ({path}) => {
    if (path.endsWith("/revisions")) {
      fixture.fleet = {...fixture.fleet, setup_revision: "2"};
      return response({fleet_id: FLEET_ID, setup_revision: "2", setup_etag: fixture.fleet.setup_etag,
        etag: fixture.fleet.etag, schedules: []}, 201);
    }
  };
  render(<WorkspaceChat {...CHAT_PROPS} fleetPage source={{kind: SOURCE_KIND.fleet, fleet_id: FLEET_ID, setup_revision: "1"}} />);
  await screen.findByRole("heading", {name: fixture.fleet.name});
  await chooseSetupAction("Instructions");
  fireEvent.change(screen.getByRole("textbox", {name: "Instructions"}), {target: {value: "Keep this fleet conversation visible."}});
  fireEvent.click(screen.getByRole("button", {name: "Save changes"}));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(fixture.fleet.setup_revision).toBe("2");
  expect(chatRouter.replace).not.toHaveBeenCalled();
  expect(screen.getByRole("heading", {name: fixture.fleet.name})).toBeTruthy();
});
