export function isPreviewCommand(command: unknown): boolean {
  return typeof command === "string" && /^(?:bun|npm|pnpm|yarn)(?: run)? preview(?:\s|$)/.test(command);
}
