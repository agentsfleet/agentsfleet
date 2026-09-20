// Whether anyone is watching, asked once.
//
// Several commands print a table to a terminal and the JSON envelope to a
// pipe, so they need to know which they have. The read belongs at the command
// boundary rather than inside a handler: a handler that reads `process.stdout`
// itself cannot be unit-tested without a global, which is why every one of
// them takes `stdoutIsTty` as an argument instead.

export const stdoutIsTty = (): boolean => Boolean(process.stdout.isTTY);
