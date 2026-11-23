import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";

import { ToolCallInputPreview } from "./FleetToolInputElements";
import { toolCopy } from "./copy/tool-call-copy";
import { TOOL_NAME } from "./presentation/tool-call-shape";

const CHANGES = [
  { path: "deploy.yaml", added: 3, removed: 1 },
  { path: "README.md", added: 0, removed: 2 },
];

afterEach(cleanup);

describe("ToolCallInputPreview", () => {
  it("lists the files an edit's recorded changes name, with their line counts", () => {
    const { container } = render(<ToolCallInputPreview copy={toolCopy(TOOL_NAME.APPLY_PATCH, { patch: "" }, CHANGES)} />);
    const files = [...container.querySelectorAll("[data-changed-file]")].map((node) => node.textContent);
    expect(files).toEqual(["deploy.yaml", "README.md"]);
    expect(container.textContent).toContain("+3");
    expect(container.textContent).toContain("+0");
  });

  it("adds nothing to the recorded JSON for a call with plain output", () => {
    const { container } = render(<ToolCallInputPreview copy={toolCopy(TOOL_NAME.HTTP_REQUEST, { url: "https://api.example.test" })} />);
    expect(container.textContent).toBe("");
  });
});
