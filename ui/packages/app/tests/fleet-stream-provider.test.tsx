import { afterEach, expect, it } from "vitest";
import { cleanup, renderHook } from "@testing-library/react";
import { useFleetStreams } from "@/components/domain/FleetStreamProvider";

afterEach(cleanup);

it("refuses fleet chat without an identity-owned stream provider", () => {
  expect(() => renderHook(useFleetStreams)).toThrowError("Fleet chat requires an identity-owned stream provider.");
});
