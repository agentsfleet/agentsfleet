// @vitest-environment node
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { PROXY_BODY_LIMIT_BYTES, SETUP_BODY_LIMIT_BYTES, TEXT_BODY_LIMIT_BYTES } from "@/lib/api/body-limits";
import nextConfig from "../next.config";

// The daemon's own statement of its two body limits, which its Rust test pins
// to the constants it serves (`afd_http::handler::setup_input`).
const SHARED: unknown = JSON.parse(readFileSync(resolve(process.cwd(), "../../../rustd/crates/daemon/afd_http/tests/fixtures/body_limits.json"), "utf8"));

/** Next's own `proxyClientMaxBodySize` default, 10 MiB. */
const NEXT_PROXY_DEFAULT_BYTES = 10 * 1024 * 1024;

describe("the app's proxy limits", () => {
  it("are the body limits the daemon reads, so neither side refuses what the other takes", () => {
    expect(SHARED).toEqual({setup_body_bytes_max: SETUP_BODY_LIMIT_BYTES, text_body_bytes_max: TEXT_BODY_LIMIT_BYTES});
  });

  it("hold the text ceiling at a client's widest escape, and a setup body holds a message beside a Definition", () => {
    expect(TEXT_BODY_LIMIT_BYTES).toBeGreaterThan(12 * 1_048_576);
    expect(SETUP_BODY_LIMIT_BYTES).toBeGreaterThan(2 * 12 * 1_048_576);
  });

  it("buffer every body a route forwards whole, past Next's default", () => {
    expect(PROXY_BODY_LIMIT_BYTES).toBeGreaterThanOrEqual(SETUP_BODY_LIMIT_BYTES);
    expect(PROXY_BODY_LIMIT_BYTES).toBeGreaterThanOrEqual(TEXT_BODY_LIMIT_BYTES);
    expect(PROXY_BODY_LIMIT_BYTES).toBeGreaterThan(NEXT_PROXY_DEFAULT_BYTES);
    expect(nextConfig.experimental?.proxyClientMaxBodySize).toBe(PROXY_BODY_LIMIT_BYTES);
  });
});
