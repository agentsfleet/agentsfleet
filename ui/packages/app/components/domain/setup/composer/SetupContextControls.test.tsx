import {afterEach, beforeEach, expect, it, vi} from "vitest";
import {act, cleanup, fireEvent, render, screen, waitFor} from "@testing-library/react";
import {useRef} from "react";
import {SetupContextControls} from "./SetupContextControls";

const draft = vi.hoisted(() => ({text: ""}));
vi.mock("@assistant-ui/react", () => ({useAui: () => ({composer: () => ({getState: () => draft, setText: (value: string) => {draft.text=value;}})})}));
beforeEach(() => { draft.text=""; });
afterEach(cleanup);
function Setup({disabled = false, mounted = true}: {disabled?: boolean; mounted?: boolean}) {
  const ref=useRef<HTMLTextAreaElement>(null);
  return <>{mounted && <textarea aria-label="instructions" ref={ref} disabled={disabled}/>}<SetupContextControls input={ref} disabled={disabled}/></>;
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
    const field = screen.getByRole<HTMLTextAreaElement>("textbox");
    draft.text=field.value=before!;
    field.setSelectionRange(field.value.length,field.value.length);
    fireEvent.click(add);
    expect(draft.text).toEqual(after);
  }
});

it("inserts at the selection and reuses a partially typed address", () => {
  const view = render(<Setup />);
  const field = screen.getByRole<HTMLTextAreaElement>("textbox");
  const add = screen.getByRole("button", {name: "Mention a fleet in setup"});
  draft.text = field.value = "Ask NAME today";
  field.focus();
  field.setSelectionRange(4, 8);
  fireEvent.click(add);
  expect(draft.text).toBe("Ask @ today");
  expect(field.selectionEnd).toBe(5);
  draft.text = field.value = "Ask @bo today";
  field.setSelectionRange(7, 7);
  fireEvent.click(add);
  expect(draft.text).toBe("Ask @bo today");
  expect(field.selectionStart).toBe(7);
  view.rerender(<Setup mounted={false} />);
  fireEvent.click(add);
  expect(draft.text).toBe("Ask @bo today");
});

it("rejects unreadable notes and preserves the draft", async () => {
  render(<Setup />);
  for(const text of ["  ","x\u0000x","\ufffd"]) {
    choose(note(text));
    await waitFor(()=>expect(screen.getByRole("status").textContent).toContain("plain text"));
  }
  draft.text="Keep this draft";
  choose({...note("ok"),text:async()=>{throw new Error("unreadable");}});
  await waitFor(()=>expect(screen.getByRole("status").textContent).toContain("Couldn’t read"));
  expect(draft.text).toBe("Keep this draft");
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
