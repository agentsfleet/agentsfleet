// A long streamed answer in the shapes a model writes: headings, prose, tight
// and loose lists, a table, and fenced code with blank lines inside it — the
// case a split on blank lines gets wrong and a real block lexer gets right.

const SECTION_COUNT = 42;
/** How many flushes the answer arrives in. */
export const FLUSH_COUNT = 400;
const FENCE = "```";

function section(index: number): string[] {
  return [
    `### Step ${index}: review the change`,
    `The runner **checked** the diff for \`service-${index}\` and read [the log](https://example.test/runs/${index}) before it wrote anything.`,
    `- read the config\n- ran the tests\n- compared the plan with the change`,
    `1. first, the migration\n\n2. then the handler\n\n3. finally the docs`,
    `| check | result |\n| --- | --- |\n| lint | pass |\n| unit | ${index} passed |`,
    `${FENCE}ts\nexport function step${index}() {\n  const a = ${index};\n\n  return a * 2;\n}\n${FENCE}`,
    `> Note: step ${index} leaves the queue as it found it.`,
  ];
}

/** Every top-level block of the answer, in order. */
export const ANSWER_BLOCKS: readonly string[] = Array.from({ length: SECTION_COUNT }, (_, index) => section(index)).flat();

/** The blank line between two top-level blocks. */
export const BLOCK_SEPARATOR = "\n\n";

/** The whole answer, about 20 KB. */
export const STREAMED_ANSWER = ANSWER_BLOCKS.join(BLOCK_SEPARATOR);

/** The longest single block: the most any one flush should have to parse, give or take a chunk. */
export const LARGEST_BLOCK = Math.max(...ANSWER_BLOCKS.map((block) => block.length));

/** The characters one flush appends. */
export const CHUNK_SIZE = Math.ceil(STREAMED_ANSWER.length / FLUSH_COUNT);

/** The answer as it stands after each flush, the last one whole. */
export function streamedPrefixes(): string[] {
  return Array.from({ length: FLUSH_COUNT }, (_, flush) =>
    STREAMED_ANSWER.slice(0, Math.min(STREAMED_ANSWER.length, (flush + 1) * CHUNK_SIZE)),
  );
}

/** Whether some flush ends inside a fence marker, so a fence arrives split across chunks. */
export function someFlushSplitsAFence(): boolean {
  return streamedPrefixes().some((prefix) => /(^|\n)`{1,2}$/.test(prefix));
}
