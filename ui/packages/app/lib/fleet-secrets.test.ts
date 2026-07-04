import { describe, expect, it } from "vitest";
import { missingSecrets, WORKSPACE_SECRETS_PATH } from "./fleet-secrets";

describe("missingSecrets", () => {
  it("returns required names absent from the workspace vault", () => {
    expect(missingSecrets(["github", "zoho"], ["github"])).toEqual(["zoho"]);
  });

  it("returns an empty list when every requirement is present", () => {
    expect(missingSecrets(["github"], ["github", "slack"])).toEqual([]);
  });

  it("returns every requirement when the vault is empty", () => {
    expect(missingSecrets(["github", "mail"], [])).toEqual(["github", "mail"]);
  });

  it("matches exactly — a different-cased vault entry does not satisfy a requirement", () => {
    expect(missingSecrets(["github"], ["GitHub"])).toEqual(["github"]);
  });
});

describe("WORKSPACE_SECRETS_PATH", () => {
  it("is the workspace credentials route, not the model-provider page", () => {
    expect(WORKSPACE_SECRETS_PATH).toBe("/secrets");
    expect(WORKSPACE_SECRETS_PATH).not.toContain("/settings/models");
  });
});
