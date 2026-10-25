import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const { useAuth, getTokenFn } = vi.hoisted(() => {
  const getTokenFn = vi.fn();
  const useAuth = vi.fn();
  return { useAuth, getTokenFn };
});

vi.mock("@clerk/nextjs", () => ({ useAuth }));

import CliAuthPage from "@/app/cli-auth/[session_id]/page";

const SESSION_ID = "0190b3c0-1111-7000-8000-000000000001";

function activeBody(overrides: Partial<{ status: string; cli_public_key: string; token_name: string; expires_at: string }> = {}) {
  return {
    status: "pending",
    cli_public_key: "AAAA",
    token_name: "darwin/arm64",
    expires_at: new Date(Date.now() + 300_000).toISOString(),
    ...overrides,
  };
}

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

async function renderPage() {
  let utils!: ReturnType<typeof render>;
  await act(async () => {
    utils = render(
      React.createElement(
        React.Suspense,
        { fallback: React.createElement("div", { "data-suspense": "1" }) },
        React.createElement(CliAuthPage, {
          params: Promise.resolve({ session_id: SESSION_ID }),
        }),
      ),
    );
  });
  await waitFor(() => {
    expect(useAuth).toHaveBeenCalled();
  });
  return utils;
}

beforeEach(() => {
  getTokenFn.mockReset();
  getTokenFn.mockResolvedValue("clerk.jwt.token");
  useAuth.mockReturnValue({ isLoaded: true, isSignedIn: true, getToken: getTokenFn });
  vi.stubGlobal("fetch", vi.fn());
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("CliAuthPage — load states", () => {
  it("shows the shared page loader while Clerk is still loading", async () => {
    useAuth.mockReturnValue({ isLoaded: false, isSignedIn: false, getToken: getTokenFn });
    vi.mocked(globalThis.fetch).mockReturnValue(
      new Promise<Response>(() => undefined),
    );
    await renderPage();
    expect(screen.getByRole("status", { name: "Loading page" }).textContent).toBe("Loading…");
    expect(screen.queryByRole("button", { name: "Authorize" })).toBeNull();
  });

  it("renders the authorization card for a pending session", async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce(jsonResponse(200, activeBody()));
    await renderPage();
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Connect your terminal" })).toBeTruthy();
    });
    expect(screen.getByRole("button", { name: "Authorize" }).hasAttribute("disabled")).toBe(false);
    expect(screen.getByText("Allow your terminal to use your agentsfleet account. Continue only if you started this sign-in.")).toBeTruthy();
  });

  it("treats verification_pending as already-approved on another tab", async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce(
      jsonResponse(200, activeBody({ status: "verification_pending" })),
    );
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText(/already been approved on another tab/)).toBeTruthy();
    });
  });

  it("disables Authorize when the session loaded as verification_pending", async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce(
      jsonResponse(200, activeBody({ status: "verification_pending" })),
    );
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText(/already been approved on another tab/)).toBeTruthy();
    });
    expect((screen.getByRole("button", { name: "Authorize" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("keeps the sign-in message readable when the terminal name contains controls", async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce(
      jsonResponse(200, activeBody({ token_name: "\x00\x01\x7f" })),
    );
    await renderPage();
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Connect your terminal" })).toBeTruthy();
    });
  });

  it("rejects an unexpected payload shape as an error state with a plain sentence", async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce(jsonResponse(200, { status: "weird" }));
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText(/went wrong loading this login session/i)).toBeTruthy();
    });
  });

  it("refuses a session that names its window as an epoch-millisecond expires_at_ms", async () => {
    const { expires_at: _instant, ...rest } = activeBody();
    vi.mocked(globalThis.fetch).mockResolvedValueOnce(
      jsonResponse(200, { ...rest, expires_at_ms: Date.now() + 300_000 }),
    );
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText(/went wrong loading this login session/i)).toBeTruthy();
    });
    expect(screen.queryByRole("button", { name: "Authorize" })).toBeNull();
  });

  it("renders 404 as 'not recognized — start over'", async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce(jsonResponse(404, { code: "not_found" }));
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText(/not recognized/)).toBeTruthy();
    });
  });

  it.each([400, 409, 410])("renders %i as 'no longer accepting approval'", async (status) => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce(jsonResponse(status, { code: "x" }));
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText(/no longer accepting approval/)).toBeTruthy();
    });
  });

  it("surfaces unexpected HTTP statuses as a plain sentence, never the raw status code", async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce(jsonResponse(503, {}));
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText(/Couldn't load this login session/i)).toBeTruthy();
    });
    expect(screen.queryByText(/HTTP 503/)).toBeNull();
  });

  it("surfaces fetch network errors", async () => {
    vi.mocked(globalThis.fetch).mockRejectedValueOnce(new TypeError("net"));
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText("Network error loading the login session.")).toBeTruthy();
    });
  });

  it("renders the sign-in card when Clerk reports unauthenticated", async () => {
    useAuth.mockReturnValue({ isLoaded: true, isSignedIn: false, getToken: getTokenFn });
    vi.mocked(globalThis.fetch).mockResolvedValueOnce(jsonResponse(200, activeBody()));
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText("Sign in to continue")).toBeTruthy();
    });
  });
});

describe("CliAuthPage — approve flow", () => {
  it("encrypts the JWT, PATCHes /approve, and displays the 6-digit code on success", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(jsonResponse(200, activeBody({ cli_public_key: await samplePeerSpkiBase64Url() })))
      .mockResolvedValueOnce(new Response(null, { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);

    await renderPage();
    await waitFor(() => expect(screen.getByText("Authorize")).toBeTruthy());

    await act(async () => {
      fireEvent.click(screen.getByText("Authorize"));
    });

    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Finish signing in" })).toBeTruthy();
      expect(screen.getByText("Go back to the terminal where you started signing in and enter this code.")).toBeTruthy();
    });
    const code = screen.getByLabelText("Verification code").textContent ?? "";
    expect(code).toMatch(/^\d{6}$/);

    const patchCall = fetchMock.mock.calls[1];
    expect(patchCall?.[0]).toContain(`/backend/v1/auth/sessions/${SESSION_ID}/approve`);
    const init = patchCall?.[1] as RequestInit;
    expect(init.method).toBe("PATCH");
    const headers = init.headers as Record<string, string>;
    expect(headers.Authorization).toBe("Bearer clerk.jwt.token");
    const body = JSON.parse(init.body as string) as {
      verification_code: string;
      ciphertext: string;
      nonce: string;
      dashboard_public_key: string;
    };
    expect(body.verification_code).toBe(code);
    expect(body.ciphertext.length).toBeGreaterThan(0);
    expect(body.nonce.length).toBeGreaterThan(0);
    expect(body.dashboard_public_key.length).toBeGreaterThan(0);
  });

  it("copies the verification code, flips the label, and resets after the timeout", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(jsonResponse(200, activeBody({ cli_public_key: await samplePeerSpkiBase64Url() })))
      .mockResolvedValueOnce(new Response(null, { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);

    await renderPage();
    await waitFor(() => expect(screen.getByText("Authorize")).toBeTruthy());
    await act(async () => {
      fireEvent.click(screen.getByText("Authorize"));
    });
    await waitFor(() => expect(screen.getByText("Copy code")).toBeTruthy());

    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      await act(async () => {
        fireEvent.click(screen.getByText("Copy code"));
      });
      await waitFor(() => expect(screen.getByText("Copied")).toBeTruthy());
      await act(async () => {
        vi.advanceTimersByTime(2500);
      });
      await waitFor(() => expect(screen.getByText("Copy code")).toBeTruthy());
    } finally {
      vi.useRealTimers();
    }
    expect(writeText).toHaveBeenCalledTimes(1);
  });

  it("falls back gracefully when clipboard.writeText throws", async () => {
    const writeText = vi.fn().mockRejectedValue(new Error("denied"));
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(jsonResponse(200, activeBody({ cli_public_key: await samplePeerSpkiBase64Url() })))
      .mockResolvedValueOnce(new Response(null, { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);

    await renderPage();
    await waitFor(() => expect(screen.getByText("Authorize")).toBeTruthy());
    await act(async () => {
      fireEvent.click(screen.getByText("Authorize"));
    });
    await waitFor(() => expect(screen.getByText("Copy code")).toBeTruthy());
    await act(async () => {
      fireEvent.click(screen.getByText("Copy code"));
    });
    expect(writeText).toHaveBeenCalled();
    expect(screen.queryByText("Copied")).toBeNull();
  });

  it("surfaces a friendly error when Clerk hands back a null token", async () => {
    getTokenFn.mockResolvedValueOnce(null);
    const fetchMock = vi.fn().mockResolvedValueOnce(
      jsonResponse(200, activeBody({ cli_public_key: await samplePeerSpkiBase64Url() })),
    );
    vi.stubGlobal("fetch", fetchMock);

    await renderPage();
    await waitFor(() => expect(screen.getByText("Authorize")).toBeTruthy());
    await act(async () => {
      fireEvent.click(screen.getByText("Authorize"));
    });
    await waitFor(() => expect(screen.getByText(/dashboard session expired/i)).toBeTruthy());
  });

  it.each([
    [409, /already approved/i],
    [410, /no longer accepting approval/i],
    [401, /not authorized/i],
    [500, /Couldn't approve this login/i],
  ])("maps PATCH /approve %i to the right error message", async (status, pattern) => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(jsonResponse(200, activeBody({ cli_public_key: await samplePeerSpkiBase64Url() })))
      .mockResolvedValueOnce(new Response(null, { status }));
    vi.stubGlobal("fetch", fetchMock);

    await renderPage();
    await waitFor(() => expect(screen.getByText("Authorize")).toBeTruthy());
    await act(async () => {
      fireEvent.click(screen.getByText("Authorize"));
    });
    await waitFor(() => expect(screen.getByText(pattern)).toBeTruthy());
    if (status === 500) expect(screen.queryByText(/HTTP 500/)).toBeNull();
  });

  it("shows the code with a caveat when the PATCH throws — the approve may have landed", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(jsonResponse(200, activeBody({ cli_public_key: await samplePeerSpkiBase64Url() })))
      .mockRejectedValueOnce(new TypeError("net"));
    vi.stubGlobal("fetch", fetchMock);

    await renderPage();
    await waitFor(() => expect(screen.getByText("Authorize")).toBeTruthy());
    await act(async () => {
      fireEvent.click(screen.getByText("Authorize"));
    });
    await waitFor(() => expect(screen.getByText(/couldn't confirm the approval/i)).toBeTruthy());
    const code = screen.getByLabelText("Verification code").textContent ?? "";
    expect(code).toMatch(/^\d{6}$/);
  });

  it("falls back to a generic failure when an earlier step throws before the code is generated", async () => {
    getTokenFn.mockRejectedValueOnce(new Error("clerk boom"));
    const fetchMock = vi.fn().mockResolvedValueOnce(
      jsonResponse(200, activeBody({ cli_public_key: await samplePeerSpkiBase64Url() })),
    );
    vi.stubGlobal("fetch", fetchMock);

    await renderPage();
    await waitFor(() => expect(screen.getByText("Authorize")).toBeTruthy());
    await act(async () => {
      fireEvent.click(screen.getByText("Authorize"));
    });
    await waitFor(() => expect(screen.getByText(/Something went wrong/i)).toBeTruthy());
    expect(screen.queryByLabelText("Verification code")).toBeNull();
  });

  it("disables Authorize after a failed attempt so a re-click can't re-trigger the round-trip", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(jsonResponse(200, activeBody({ cli_public_key: await samplePeerSpkiBase64Url() })))
      .mockResolvedValueOnce(new Response(null, { status: 409 }));
    vi.stubGlobal("fetch", fetchMock);

    await renderPage();
    await waitFor(() => expect(screen.getByText("Authorize")).toBeTruthy());
    await act(async () => {
      fireEvent.click(screen.getByText("Authorize"));
    });
    await waitFor(() => expect(screen.getByText(/already approved/i)).toBeTruthy());
    expect((screen.getByRole("button", { name: "Authorize" }) as HTMLButtonElement).disabled).toBe(true);
  });
});

async function samplePeerSpkiBase64Url(): Promise<string> {
  const pair = await crypto.subtle.generateKey(
    { name: "ECDH", namedCurve: "P-256" },
    true,
    ["deriveBits"],
  );
  const spki = await crypto.subtle.exportKey("spki", pair.publicKey);
  const bytes = new Uint8Array(spki);
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}
