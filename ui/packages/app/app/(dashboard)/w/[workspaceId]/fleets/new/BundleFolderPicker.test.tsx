import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { BundleFolderPicker } from "./BundleFolderPicker";
import { SKILL_FILE_NAME, TRIGGER_FILE_NAME } from "./bundle-files";

const SKILL_BODY = "---\nname: incident-repairer\n---\nBody.";
const TRIGGER_BODY = "---\nname: incident-repairer\nx-agentsfleet:\n---";
const BUNDLE_DIR = "incident-repairer";
const CHOOSE_FOLDER_LABEL = "Choose bundle folder";

afterEach(() => cleanup());

function pickedFile(relativePath: string, body: string): File {
  const name = relativePath.slice(relativePath.lastIndexOf("/") + 1);
  const file = new File([body], name, { type: "text/markdown" });
  Object.defineProperty(file, "webkitRelativePath", { value: relativePath });
  return file;
}

function renderPicker() {
  const onLoaded = vi.fn();
  render(React.createElement(BundleFolderPicker, { onLoaded }));
  return { onLoaded, input: screen.getByLabelText(CHOOSE_FOLDER_LABEL) };
}

describe("BundleFolderPicker", () => {
  it("offers a directory picker, not a file one", () => {
    const { input } = renderPicker();
    // Without this attribute the control opens a file chooser, which is the
    // one thing this surface does not accept.
    expect(input.hasAttribute("webkitdirectory")).toBe(true);
    expect(screen.getByText(new RegExp(`Reads ${SKILL_FILE_NAME} and ${TRIGGER_FILE_NAME}`))).toBeTruthy();
  });

  it("hands both bodies up when a chosen folder holds a whole bundle", async () => {
    const { onLoaded, input } = renderPicker();
    fireEvent.change(input, {
      target: {
        files: [
          pickedFile(`${BUNDLE_DIR}/${SKILL_FILE_NAME}`, SKILL_BODY),
          pickedFile(`${BUNDLE_DIR}/${TRIGGER_FILE_NAME}`, TRIGGER_BODY),
        ],
      },
    });

    expect(await screen.findByText(`Loaded ${SKILL_FILE_NAME} and ${TRIGGER_FILE_NAME}.`)).toBeTruthy();
    expect(onLoaded).toHaveBeenCalledWith(SKILL_BODY, TRIGGER_BODY);
  });

  it("shows the refusal and hands nothing up when the folder is incomplete", async () => {
    const { onLoaded, input } = renderPicker();
    fireEvent.change(input, {
      target: { files: [pickedFile(`${BUNDLE_DIR}/${SKILL_FILE_NAME}`, SKILL_BODY)] },
    });

    expect(await screen.findByText(new RegExp(`has no ${TRIGGER_FILE_NAME}`))).toBeTruthy();
    expect(onLoaded).not.toHaveBeenCalled();
  });

  it("says so when the files went stale between the pick and the read", async () => {
    // File.text() rejects when the file was edited, moved or deleted after the
    // pick. Swallowing it would leave the previous pick's bodies in the boxes
    // under a "Loaded" line that no longer describes them.
    const { onLoaded, input } = renderPicker();
    const skill = pickedFile(`${BUNDLE_DIR}/${SKILL_FILE_NAME}`, SKILL_BODY);
    skill.text = () => Promise.reject(new Error("NotReadableError"));

    fireEvent.change(input, {
      target: { files: [skill, pickedFile(`${BUNDLE_DIR}/${TRIGGER_FILE_NAME}`, TRIGGER_BODY)] },
    });

    expect(await screen.findByText(/could not be read/i)).toBeTruthy();
    expect(onLoaded).not.toHaveBeenCalled();
    // The control must not be wedged shut by the failure.
    expect((input as HTMLInputElement).value).toBe("");
  });

  it("replaces a refusal when the next pick succeeds", async () => {
    const { onLoaded, input } = renderPicker();
    fireEvent.change(input, {
      target: { files: [pickedFile(`${BUNDLE_DIR}/${SKILL_FILE_NAME}`, SKILL_BODY)] },
    });
    await screen.findByText(new RegExp(`has no ${TRIGGER_FILE_NAME}`));

    fireEvent.change(input, {
      target: {
        files: [
          pickedFile(`${BUNDLE_DIR}/${SKILL_FILE_NAME}`, SKILL_BODY),
          pickedFile(`${BUNDLE_DIR}/${TRIGGER_FILE_NAME}`, TRIGGER_BODY),
        ],
      },
    });

    expect(await screen.findByText(`Loaded ${SKILL_FILE_NAME} and ${TRIGGER_FILE_NAME}.`)).toBeTruthy();
    expect(screen.queryByText(new RegExp(`has no ${TRIGGER_FILE_NAME}`))).toBeNull();
    expect(onLoaded).toHaveBeenCalledWith(SKILL_BODY, TRIGGER_BODY);
  });

  it("stays quiet when the picker is cancelled", async () => {
    const { onLoaded, input } = renderPicker();
    fireEvent.change(input, { target: { files: [] } });

    // A cancelled picker is not a mistake, so the hint stays and nothing is said.
    expect(await screen.findByText(new RegExp(`Reads ${SKILL_FILE_NAME}`))).toBeTruthy();
    expect(onLoaded).not.toHaveBeenCalled();
  });
});
