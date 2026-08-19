import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { Button, Form } from "@agentsfleet/design-system";

import { GitHubSourceField, LibrarySourceTabs } from "./LibrarySourceTabs";
import { SKILL_FILE_NAME, TRIGGER_FILE_NAME } from "./bundle-files";
import {
  EMPTY_LIBRARY_SOURCE,
  librarySourceSchema,
  type LibrarySourceValues,
} from "./library-source-form";

const REPO = "agentsfleet/github-pr-reviewer";
const SKILL_BODY = "---\nname: incident-responder\n---\nBody.";
const TRIGGER_BODY = "---\nname: incident-responder\nx-agentsfleet:\n---";
const BUNDLE_DIR = "incident-responder";
const CHOOSE_FOLDER_LABEL = "Choose bundle folder";
const UPLOAD_TAB = "Upload from computer";
const GITHUB_TAB = "GitHub";

afterEach(() => cleanup());

function pickedFile(relativePath: string, body: string): File {
  const name = relativePath.slice(relativePath.lastIndexOf("/") + 1);
  const file = new File([body], name, { type: "text/markdown" });
  Object.defineProperty(file, "webkitRelativePath", { value: relativePath });
  return file;
}

// Stands in for either dialog: it owns the form, exactly as both real callers do,
// and reports the values back so the tabs' effect on them is observable.
function Harness({
  onSourceChange,
  readOnlyField = false,
  seen,
}: {
  onSourceChange?: () => void;
  readOnlyField?: boolean;
  seen?: (values: LibrarySourceValues) => void;
}) {
  const form = useForm<LibrarySourceValues>({
    resolver: zodResolver(librarySourceSchema),
    defaultValues: EMPTY_LIBRARY_SOURCE,
  });
  seen?.(form.watch());
  return (
    <Form {...form}>
      {readOnlyField ? (
        <GitHubSourceField form={form} readOnly />
      ) : (
        <LibrarySourceTabs form={form} onSourceChange={onSourceChange} />
      )}
    </Form>
  );
}

// The plain harness never submits, so zod never runs and no field refusal
// exists to clear. This one does, which is what makes the two clearing tests
// above assert something rather than pass vacuously.
function HarnessWithSubmit() {
  const form = useForm<LibrarySourceValues>({
    resolver: zodResolver(librarySourceSchema),
    defaultValues: EMPTY_LIBRARY_SOURCE,
  });
  return (
    <Form {...form}>
      <form onSubmit={(e) => { void form.handleSubmit(() => {})(e); }}>
        <LibrarySourceTabs form={form} />
        <Button type="submit">submit</Button>
      </form>
    </Form>
  );
}

function renderTabs(props: Partial<React.ComponentProps<typeof Harness>> = {}) {
  const values: LibrarySourceValues[] = [];
  render(<Harness seen={(v) => values.push(v)} {...props} />);
  return { latest: () => values[values.length - 1] };
}

describe("LibrarySourceTabs", () => {
  it("opens on GitHub, the source that needs nothing from the operator's disk", () => {
    renderTabs();
    expect(screen.getByLabelText("Repository")).toBeTruthy();
    expect(screen.queryByLabelText(SKILL_FILE_NAME)).toBeNull();
  });

  it("offers the local bundle as the second source", async () => {
    const user = userEvent.setup();
    const { latest } = renderTabs();

    await user.click(screen.getByRole("tab", { name: UPLOAD_TAB }));

    expect(await screen.findByLabelText(SKILL_FILE_NAME)).toBeTruthy();
    expect(screen.getByLabelText(TRIGGER_FILE_NAME)).toBeTruthy();
    expect(screen.getByLabelText(CHOOSE_FOLDER_LABEL)).toBeTruthy();
    expect(latest()?.source_kind).toBe("upload");
  });

  // The caller holds the server error, so only the caller can clear it — and it
  // must, or an upload attempt keeps explaining a failure the GitHub tab earned.
  it("tells the caller when the operator changes source", async () => {
    const user = userEvent.setup();
    const onSourceChange = vi.fn();
    renderTabs({ onSourceChange });

    await user.click(screen.getByRole("tab", { name: UPLOAD_TAB }));
    await user.click(screen.getByRole("tab", { name: GITHUB_TAB }));

    expect(onSourceChange).toHaveBeenCalledTimes(2);
  });

  it("keeps a typed repository while the operator looks at the other tab", async () => {
    const user = userEvent.setup();
    const { latest } = renderTabs();

    await user.type(screen.getByLabelText("Repository"), REPO);
    await user.click(screen.getByRole("tab", { name: UPLOAD_TAB }));
    await user.click(screen.getByRole("tab", { name: GITHUB_TAB }));

    expect(((await screen.findByLabelText("Repository")) as HTMLInputElement).value).toBe(REPO);
    expect(latest()?.source_ref).toBe(REPO);
  });

  // The folder fills the boxes rather than going straight to the wire: frontmatter
  // is unforgiving, so the person uploading gets to read what leaves the browser.
  it("fills both bodies from a chosen bundle folder", async () => {
    const user = userEvent.setup();
    const { latest } = renderTabs();
    await user.click(screen.getByRole("tab", { name: UPLOAD_TAB }));

    fireEvent.change(screen.getByLabelText(CHOOSE_FOLDER_LABEL), {
      target: {
        files: [
          pickedFile(`${BUNDLE_DIR}/${SKILL_FILE_NAME}`, SKILL_BODY),
          pickedFile(`${BUNDLE_DIR}/${TRIGGER_FILE_NAME}`, TRIGGER_BODY),
        ],
      },
    });

    await waitFor(() =>
      expect((screen.getByLabelText(SKILL_FILE_NAME) as HTMLTextAreaElement).value).toBe(SKILL_BODY),
    );
    expect((screen.getByLabelText(TRIGGER_FILE_NAME) as HTMLTextAreaElement).value).toBe(TRIGGER_BODY);
    expect(latest()?.trigger_markdown).toBe(TRIGGER_BODY);
  });

  // A refusal the other tab earned explains a field this one does not have.
  it("drops a field refusal when the operator changes source", async () => {
    const user = userEvent.setup();
    render(<HarnessWithSubmit />);

    await user.type(screen.getByLabelText("Repository"), "notarepo");
    await user.click(screen.getByRole("button", { name: /^submit$/i }));
    expect(await screen.findByText(/use owner\/repo/i)).toBeTruthy();

    await user.click(screen.getByRole("tab", { name: UPLOAD_TAB }));

    await waitFor(() => expect(screen.queryByText(/use owner\/repo/i)).toBeNull());
  });

  // The refusal names the empty box; a folder that fills it has answered it.
  it("drops the missing-body refusal once a folder fills the boxes", async () => {
    const user = userEvent.setup();
    render(<HarnessWithSubmit />);
    await user.click(screen.getByRole("tab", { name: UPLOAD_TAB }));
    await user.click(screen.getByRole("button", { name: /^submit$/i }));
    expect(await screen.findByText(`Add the ${SKILL_FILE_NAME} body`)).toBeTruthy();

    fireEvent.change(screen.getByLabelText(CHOOSE_FOLDER_LABEL), {
      target: {
        files: [
          pickedFile(`${BUNDLE_DIR}/${SKILL_FILE_NAME}`, SKILL_BODY),
          pickedFile(`${BUNDLE_DIR}/${TRIGGER_FILE_NAME}`, TRIGGER_BODY),
        ],
      },
    });

    await waitFor(() =>
      expect(screen.queryByText(`Add the ${SKILL_FILE_NAME} body`)).toBeNull(),
    );
  });

  it("links the authoring guide and a real importable example from the GitHub tab", () => {
    renderTabs();
    expect(
      screen.getByRole("link", { name: /^learn more/i }).getAttribute("href"),
    ).toBe("https://docs.agentsfleet.net/fleets/authoring");
    expect(screen.getByRole("link", { name: new RegExp(`^${REPO}`) }).getAttribute("href")).toBe(
      `https://github.com/${REPO}`,
    );
  });
});

describe("GitHubSourceField", () => {
  // The catalog's Fetch-update re-reads a row's own source. Offering an editable
  // one would offer to change what is being re-read, which is a different write.
  it("renders read-only when the source is already decided", () => {
    renderTabs({ readOnlyField: true });
    expect((screen.getByLabelText("Repository") as HTMLInputElement).readOnly).toBe(true);
    expect(screen.queryByRole("tab", { name: UPLOAD_TAB })).toBeNull();
  });
});
