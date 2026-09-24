// Grade each LCOV source block from named functions when Bun emits them,
// otherwise from that block's FNF/FNH totals. Bun can mix both shapes in one
// run, so choosing a single format for the entire file can hide misses.

const parseCount = (raw) => {
  if (!/^(0|[1-9][0-9]*)$/.test(raw)) throw new Error("invalid LCOV count");
  const value = Number(raw);
  if (!Number.isSafeInteger(value)) throw new Error("invalid LCOV count");
  return value;
};

export function gradeLcov(raw) {
  let functionsFound = 0;
  let functionsHit = 0;
  let linesFound = 0;
  let linesHit = 0;
  let source = null;
  let namedFunctions = new Set();
  let namedHits = new Set();
  let totalFunctions = 0;
  let totalHits = 0;
  const uncovered = [];

  const flushBlock = () => {
    if (source === null) return;
    // A full named set is more precise than FNH, which has disagreed with
    // FNDA on some Bun runs. An incomplete named set uses the block totals.
    if (namedFunctions.size > 0 && (totalFunctions === 0 || namedFunctions.size >= totalFunctions)) {
      functionsFound += namedFunctions.size;
      for (const name of namedHits) if (namedFunctions.has(name)) functionsHit += 1;
    } else {
      functionsFound += totalFunctions;
      functionsHit += totalHits;
    }
    namedFunctions = new Set();
    namedHits = new Set();
    totalFunctions = 0;
    totalHits = 0;
  };

  for (const line of raw.split("\n")) {
    if (line.startsWith("SF:")) { flushBlock(); source = line.slice(3); }
    else if (line.startsWith("FN:")) namedFunctions.add(line.slice(3).split(",").slice(1).join(","));
    else if (line.startsWith("FNDA:")) {
      const [count, ...nameParts] = line.slice(5).split(",");
      if (parseCount(count) > 0) namedHits.add(nameParts.join(","));
    }
    else if (line.startsWith("FNF:")) totalFunctions = parseCount(line.slice(4));
    else if (line.startsWith("FNH:")) totalHits = parseCount(line.slice(4));
    else if (line.startsWith("LF:")) linesFound += parseCount(line.slice(3));
    else if (line.startsWith("LH:")) linesHit += parseCount(line.slice(3));
    else if (line.startsWith("DA:") && source !== null) {
      const [number, count] = line.slice(3).split(",");
      if (parseCount(count) === 0) uncovered.push(`${source}:${number}`);
    }
  }
  flushBlock();

  if (linesFound === 0) throw new Error("lcov.info carried no line records");
  if (functionsFound === 0) throw new Error("lcov.info carried no function records");
  if (![functionsFound, functionsHit, linesFound, linesHit].every(Number.isSafeInteger)) {
    throw new Error("lcov.info count sum exceeded the safe integer range");
  }
  if (functionsHit > functionsFound || linesHit > linesFound) {
    throw new Error("lcov.info carried hit counts greater than found counts");
  }
  return {
    fn: (functionsHit / functionsFound) * 100,
    line: (linesHit / linesFound) * 100,
    uncovered,
  };
}
