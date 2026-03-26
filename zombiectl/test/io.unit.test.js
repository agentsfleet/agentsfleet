import { describe, test, expect } from "bun:test";
import { makeBufferStream, ui } from "./helpers.js";
import { printHelp } from "../src/program/io.js";

describe("printHelp authRole option", () => {
  test("authRole=operator shows operator commands", () => {
    const out = makeBufferStream();
    printHelp(out.stream, ui, { authRole: "operator" });
    const output = out.read();
    expect(output).toContain("OPERATOR COMMANDS");
    expect(output).toContain("workspace upgrade-scale");
  });

  test("authRole=admin shows operator commands", () => {
    const out = makeBufferStream();
    printHelp(out.stream, ui, { authRole: "admin" });
    const output = out.read();
    expect(output).toContain("OPERATOR COMMANDS");
    expect(output).toContain("workspace upgrade-scale");
  });

  test("authRole=user does NOT show operator commands", () => {
    const out = makeBufferStream();
    printHelp(out.stream, ui, { authRole: "user" });
    const output = out.read();
    expect(output).not.toContain("OPERATOR COMMANDS");
    expect(output).not.toContain("workspace upgrade-scale");
  });

  test("no authRole and no ZOMBIE_OPERATOR hides operator commands", () => {
    const out = makeBufferStream();
    printHelp(out.stream, ui, {});
    const output = out.read();
    expect(output).not.toContain("OPERATOR COMMANDS");
  });

  test("ZOMBIE_OPERATOR=1 still works as env override", () => {
    const out = makeBufferStream();
    printHelp(out.stream, ui, { env: { ZOMBIE_OPERATOR: "1" } });
    const output = out.read();
    expect(output).toContain("OPERATOR COMMANDS");
  });

  test("operator commands section includes workspace upgrade-scale", () => {
    const out = makeBufferStream();
    printHelp(out.stream, ui, { operator: true });
    const output = out.read();
    expect(output).toContain("workspace upgrade-scale --workspace-id ID --subscription-id SUBSCRIPTION_ID");
  });

  test("ZOMBIE_OPERATOR description says force-show", () => {
    const out = makeBufferStream();
    printHelp(out.stream, ui, {});
    const output = out.read();
    expect(output).toContain("force-show operator commands");
  });
});
