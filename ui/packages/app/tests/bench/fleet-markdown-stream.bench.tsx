import { describe, expect, test } from "vitest";
import { render } from "@testing-library/react";

import { FleetMarkdown, FleetStreamingMarkdown } from "@/components/domain/FleetMarkdown";
import { streamedPrefixes } from "@/tests/helpers/fleet-markdown-corpus";

// One streamed reply, rendered flush by flush through each path: the whole
// prefix re-parsed every flush, and the block path that re-parses only its
// open tail. Run with `bunx vitest bench --run --reporter=verbose tests/bench`
// (the default reporter prints the table only when the comparison fails).

const PREFIXES = streamedPrefixes();
const ITERATIONS = 5;
// One warmup pass: the whole-prefix path takes seconds per pass, and the
// default warmup multiplies that for no steadier a mean.
const WARMUP_ITERATIONS = 1;
// Six passes of the slow path at up to 25 s each, with room to spare.
const BENCH_TIMEOUT_MS = 300_000;
const WHOLE_PREFIX = "whole prefix: the answer so far, parsed every flush";
const BLOCKS = "block: finished blocks parsed once, the open tail every flush";

function streamWholePrefix() {
  const view = render(<FleetMarkdown>{""}</FleetMarkdown>);
  for (const prefix of PREFIXES) view.rerender(<FleetMarkdown>{prefix}</FleetMarkdown>);
  view.unmount();
}

function streamBlocks() {
  const view = render(<FleetStreamingMarkdown text="" />);
  for (const prefix of PREFIXES) view.rerender(<FleetStreamingMarkdown text={prefix} />);
  view.unmount();
}

describe("a 20 KB answer streamed in 400 flushes", () => {
  test("renders flush by flush faster block by block than whole", { timeout: BENCH_TIMEOUT_MS }, async ({ bench }) => {
    const result = await bench.compare(bench(WHOLE_PREFIX, streamWholePrefix), bench(BLOCKS, streamBlocks), {
      iterations: ITERATIONS,
      time: 0,
      warmupIterations: WARMUP_ITERATIONS,
      warmupTime: 0,
    });
    expect(result.get(BLOCKS)).toBeFasterThan(result.get(WHOLE_PREFIX));
  });
});
