import { describe, expect, it } from "vitest";
import { missingCredentials, WORKSPACE_CREDENTIALS_PATH } from "./fleet-credentials";

describe("missingCredentials", () => {
  it("returns required names absent from the workspace vault", () => {
    expect(missingCredentials(["github", "zoho"], ["github"])).toEqual(["zoho"]);
  });

  it("returns an empty list when every requirement is present", () => {
    expect(missingCredentials(["github"], ["github", "slack"])).toEqual([]);
  });

  it("returns every requirement when the vault is empty", () => {
    expect(missingCredentials(["github", "mail"], [])).toEqual(["github", "mail"]);
  });

  it("matches exactly — a different-cased vault entry does not satisfy a requirement", () => {
    expect(missingCredentials(["github"], ["GitHub"])).toEqual(["github"]);
  });
});

describe("WORKSPACE_CREDENTIALS_PATH", () => {
  it("is the workspace credentials route, not the model-provider page", () => {
    expect(WORKSPACE_CREDENTIALS_PATH).toBe("/credentials");
    expect(WORKSPACE_CREDENTIALS_PATH).not.toContain("/settings/models");
  });
});
