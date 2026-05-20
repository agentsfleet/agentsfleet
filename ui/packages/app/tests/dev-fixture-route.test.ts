import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

const notFound = vi.fn(() => {
  throw new Error("notFound");
});
vi.mock("next/navigation", () => ({ notFound }));

afterEach(() => {
  vi.unstubAllEnvs();
  vi.clearAllMocks();
});

describe("ds-button-rsc build fixture route", () => {
  it("renders the Button RSC fixture outside production", async () => {
    vi.stubEnv("NODE_ENV", "development");
    const { default: Page } = await import("../app/(dev)/ds-button-rsc/page");
    const markup = renderToStaticMarkup(React.createElement(Page));
    expect(markup).toContain("RSC fixture");
    expect(notFound).not.toHaveBeenCalled();
  });

  it("404s in production so the fixture is not a public surface", async () => {
    vi.stubEnv("NODE_ENV", "production");
    const { default: Page } = await import("../app/(dev)/ds-button-rsc/page");
    expect(() => renderToStaticMarkup(React.createElement(Page))).toThrow(
      "notFound",
    );
    expect(notFound).toHaveBeenCalledOnce();
  });
});
