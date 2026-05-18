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

function activeBody(overrides: Partial<{ status: string; cli_public_key: string; token_name: string; expires_at_ms: number }> = {}) {
  return {
    status: "pending",
    cli_public_key: "AAAA",
    token_name: "darwin/arm64",
    expires_at_ms: Date.now() + 300_000,
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
  it("shows the loading skeleton while Clerk is still loading", async () => {
    useAuth.mockReturnValue({ isLoaded: false, isSignedIn: false, getToken: getTokenFn });
    (globalThis.fetch as ReturnType<typeof vi.fn>).mockReturnValue(
      new Promise<Response>(() => undefined),
    );
    await renderPage();
    expect(screen.getByText("Approve CLI login")).toBeTruthy();
    expect(screen.getByText(/Checking your terminal/)).toBeTruthy();
  });

  it("renders the Approve card for a pending session", async () => {
    (globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce(jsonResponse(200, activeBody()));
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText(/Approve CLI login for/)).toBeTruthy();
    });
    expect(screen.getByText("Approve")).toBeTruthy();
  });

  it("treats verification_pending as already-approved on another tab", async () => {
    (globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      jsonResponse(200, activeBody({ status: "verification_pending" })),
    );
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText(/already been approved on another tab/)).toBeTruthy();
    });
  });

  it("falls back to 'your terminal' when token_name is blank/control-only", async () => {
    (globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      jsonResponse(200, activeBody({ token_name: "\x00\x01\x7f" })),
    );
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText("Approve CLI login for your terminal")).toBeTruthy();
    });
  });

  it("rejects an unexpected payload shape as an error state", async () => {
    (globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce(jsonResponse(200, { status: "weird" }));
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText("Unexpected session payload.")).toBeTruthy();
    });
  });

  it("renders 404 as 'not recognized — start over'", async () => {
    (globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce(jsonResponse(404, { code: "not_found" }));
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText(/not recognized/)).toBeTruthy();
    });
  });

  it.each([400, 409, 410])("renders %i as 'no longer accepting approval'", async (status) => {
    (globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce(jsonResponse(status, { code: "x" }));
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText(/no longer accepting approval/)).toBeTruthy();
    });
  });

  it("surfaces unexpected HTTP statuses with the code in the message", async () => {
    (globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce(jsonResponse(503, {}));
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText(/HTTP 503/)).toBeTruthy();
    });
  });

  it("surfaces fetch network errors", async () => {
    (globalThis.fetch as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new TypeError("net"));
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText("Network error loading the login session.")).toBeTruthy();
    });
  });

  it("ignores the in-flight GET when the component unmounts before the response", async () => {
    let resolveFetch: (r: Response) => void = () => undefined;
    const pending = new Promise<Response>((r) => {
      resolveFetch = r;
    });
    (globalThis.fetch as ReturnType<typeof vi.fn>).mockReturnValueOnce(pending);
    const { unmount } = await renderPage();
    unmount();
    resolveFetch(jsonResponse(200, activeBody()));
    await new Promise((r) => setTimeout(r, 10));
    // No assertion needed beyond "did not throw" — covers the `cancelled` branch.
  });

  it("ignores a rejected in-flight GET after unmount", async () => {
    let rejectFetch: (e: unknown) => void = () => undefined;
    const pending = new Promise<Response>((_resolve, reject) => {
      rejectFetch = reject;
    });
    (globalThis.fetch as ReturnType<typeof vi.fn>).mockReturnValueOnce(pending);
    const { unmount } = await renderPage();
    unmount();
    rejectFetch(new TypeError("net"));
    await new Promise((r) => setTimeout(r, 10));
    // Covers the catch-arm `if (!cancelled)` branch's cancelled-true path.
  });

  it("renders the sign-in card when Clerk reports unauthenticated", async () => {
    useAuth.mockReturnValue({ isLoaded: true, isSignedIn: false, getToken: getTokenFn });
    (globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce(jsonResponse(200, activeBody()));
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
    await waitFor(() => expect(screen.getByText("Approve")).toBeTruthy());

    await act(async () => {
      fireEvent.click(screen.getByText("Approve"));
    });

    await waitFor(() => {
      expect(screen.getByText("Type this code into your CLI")).toBeTruthy();
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
    await waitFor(() => expect(screen.getByText("Approve")).toBeTruthy());
    await act(async () => {
      fireEvent.click(screen.getByText("Approve"));
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
    await waitFor(() => expect(screen.getByText("Approve")).toBeTruthy());
    await act(async () => {
      fireEvent.click(screen.getByText("Approve"));
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
    await waitFor(() => expect(screen.getByText("Approve")).toBeTruthy());
    await act(async () => {
      fireEvent.click(screen.getByText("Approve"));
    });
    await waitFor(() => expect(screen.getByText(/dashboard session expired/i)).toBeTruthy());
  });

  it.each([
    [409, /already approved/i],
    [410, /no longer accepting approval/i],
    [401, /not authorized/i],
    [500, /HTTP 500/i],
  ])("maps PATCH /approve %i to the right error message", async (status, pattern) => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(jsonResponse(200, activeBody({ cli_public_key: await samplePeerSpkiBase64Url() })))
      .mockResolvedValueOnce(new Response(null, { status }));
    vi.stubGlobal("fetch", fetchMock);

    await renderPage();
    await waitFor(() => expect(screen.getByText("Approve")).toBeTruthy());
    await act(async () => {
      fireEvent.click(screen.getByText("Approve"));
    });
    await waitFor(() => expect(screen.getByText(pattern)).toBeTruthy());
  });

  it("surfaces a generic error when the PATCH itself throws", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(jsonResponse(200, activeBody({ cli_public_key: await samplePeerSpkiBase64Url() })))
      .mockRejectedValueOnce(new TypeError("net"));
    vi.stubGlobal("fetch", fetchMock);

    await renderPage();
    await waitFor(() => expect(screen.getByText("Approve")).toBeTruthy());
    await act(async () => {
      fireEvent.click(screen.getByText("Approve"));
    });
    await waitFor(() => expect(screen.getByText(/Something went wrong/i)).toBeTruthy());
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
