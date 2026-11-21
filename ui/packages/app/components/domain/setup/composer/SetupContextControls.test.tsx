import {afterEach, beforeEach, expect, it, vi} from "vitest";
import {act, cleanup, fireEvent, render, screen, waitFor} from "@testing-library/react";
import {useRef} from "react";
import {MESSAGE_MAX_BYTES} from "@/lib/api/draft-types";
import {SetupContextControls} from "./SetupContextControls";

const draft = vi.hoisted(() => ({text: ""}));
vi.mock("@assistant-ui/react", () => ({useAui: () => ({composer: () => ({getState: () => draft, setText: (value: string) => {draft.text=value;}})})}));
beforeEach(() => { draft.text=""; });
afterEach(cleanup);
function Setup({disabled = false}: {disabled?: boolean}) {
  const ref=useRef<HTMLTextAreaElement>(null);
  return <><textarea aria-label="instructions" ref={ref} disabled={disabled}/><SetupContextControls input={ref} disabled={disabled}/></>;
}
function choose(file?: Pick<File,"size"|"name"|"text">) {
  fireEvent.change(screen.getByLabelText("Choose a text document"), {target: {files: file ? [file] : []}});
}
const note=(text: string) => ({name:"notes.md",size:text.length,text:async()=>text});

it("inserts readable text into the editable draft", async () => {
  render(<Setup />);
  fireEvent.click(screen.getByRole("button",{name:"Insert a text document into setup"}));
  choose();
  choose(note("Daily report"));
  await waitFor(()=>expect(draft.text).toBe("notes.md\nDaily report"));
  choose(note("More"));
  await waitFor(()=>expect(draft.text).toBe("notes.md\nDaily report\n\nnotes.md\nMore"));
  expect(document.activeElement).toBe(screen.getByRole("textbox"));
});

it("opens a mention from empty and nonempty setup instructions", () => {
  render(<Setup />);
  const add=screen.getByRole("button",{name:"Mention a fleet in setup"});
  for(const [before,after] of [["","@"],["Ask","Ask @"],["Ask ","Ask @"]]) {
    draft.text=before!;
    fireEvent.click(add);
    expect(draft.text).toEqual(after);
  }
});

it("rejects unreadable or oversized notes and preserves the draft", async () => {
  render(<Setup />);
  for(const text of ["  ","x\u0000x","\ufffd"]) {
    choose(note(text));
    await waitFor(()=>expect(screen.getByRole("status").textContent).toContain("plain text"));
  }
  choose({...note("ok"),size:MESSAGE_MAX_BYTES+1});
  await waitFor(()=>expect(screen.getByRole("status").textContent).toContain("32 KiB"));
  draft.text="a".repeat(MESSAGE_MAX_BYTES);
  choose(note("one"));
  await waitFor(()=>expect(screen.getByRole("status").textContent).toContain("32 KiB"));
  choose({...note("ok"),text:async()=>{throw new Error("unreadable");}});
  await waitFor(()=>expect(screen.getByRole("status").textContent).toContain("Couldn’t read"));
  expect(draft.text.length).toBe(MESSAGE_MAX_BYTES);
});

it.each(["edited","unmounted","disabled"])("preserves a draft %s during the file read", async state => {
  const view=render(<Setup />);
  const read=Promise.withResolvers<string>();
  choose({...note("x"),text:()=>read.promise});
  if(state==="edited") draft.text="Other instructions";
  if(state==="unmounted") view.unmount();
  if(state==="disabled") view.rerender(<Setup disabled />);
  await act(async()=>read.resolve("notes"));
  expect(draft.text).toBe(state==="edited" ? "Other instructions" : "");
  if(state==="edited") expect(screen.getByRole("status").textContent).toContain("draft changed");
});
