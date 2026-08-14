import { describe, test, expect } from "bun:test";
import {
  requireAuth,
  unboundTarget,
  AUTH_FAIL_MESSAGE,
} from "../src/program/auth-guard.ts";

const AFC = `afc_${"a".repeat(64)}`;

describe("requireAuth", () => {
  test("token present returns ok", () => {
    const result = requireAuth({ token: "header.payload.sig", apiKey: null });
    expect(result.ok).toBe(true);
  });

  test("API key present returns ok", () => {
    const result = requireAuth({ token: null, apiKey: "sk-test-123" });
    expect(result.ok).toBe(true);
  });

  test("both present returns ok", () => {
    const result = requireAuth({ token: "tok", apiKey: "key" });
    expect(result.ok).toBe(true);
  });

  test("neither present returns fail", () => {
    const result = requireAuth({ token: null, apiKey: null });
    expect(result.ok).toBe(false);
  });

  test("empty strings treated as falsy", () => {
    const result = requireAuth({ token: "", apiKey: "" });
    expect(result.ok).toBe(false);
  });

  test("AUTH_FAIL_MESSAGE contains login instruction", () => {
    expect(AUTH_FAIL_MESSAGE).toContain("agentsfleet login");
  });
});

// The one case that would otherwise be a guess: no stored deployment AND no
// named target, so the ladder falls to the production default and reaches it
// in silence.
describe("unboundTarget", () => {
  test("test_unbound_credential_refused_when_target_is_inferred", () => {
    const refusal = unboundTarget({
      token: AFC,
      apiKey: null,
      apiUrl: "https://api.agentsfleet.net",
      storedApiUrl: null,
      targetIsExplicit: false,
    });
    expect(refusal).not.toBeNull();
    expect(refusal).toContain("--api");
    expect(refusal).toContain("AGENTSFLEET_API_URL");
    expect(refusal).toContain("agentsfleet login");
  });

  test("a named target always wins — the operator said where to go", () => {
    expect(
      unboundTarget({
        token: AFC,
        apiKey: null,
        apiUrl: "https://api.agentsfleet.net",
        storedApiUrl: null,
        targetIsExplicit: true,
      }),
    ).toBeNull();
  });

  test("a stored deployment IS what the ladder resolved, so it cannot disagree", () => {
    expect(
      unboundTarget({
        token: AFC,
        apiKey: null,
        apiUrl: "https://api-dev.agentsfleet.net",
        storedApiUrl: "https://api-dev.agentsfleet.net",
        targetIsExplicit: false,
      }),
    ).toBeNull();
  });

  test("env API key is the credential in play, so the stored one is not judged", () => {
    expect(
      unboundTarget({
        token: AFC,
        apiKey: "agt_t_service_key",
        apiUrl: "https://api.agentsfleet.net",
        storedApiUrl: null,
        targetIsExplicit: false,
      }),
    ).toBeNull();
  });

  test("no credential at all is not this guard's business", () => {
    expect(
      unboundTarget({
        token: null,
        apiKey: null,
        apiUrl: "https://api.agentsfleet.net",
        storedApiUrl: null,
        targetIsExplicit: false,
      }),
    ).toBeNull();
  });
});
