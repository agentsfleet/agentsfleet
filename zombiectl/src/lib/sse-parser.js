/**
 * Parse SSE frames from a text buffer.
 * Returns { events: [...], remainder: string }.
 * Each event has { type, data }.
 */
export function parseSseBuffer(buf) {
  const events = [];
  let remainder = buf;

  while (true) {
    const boundary = remainder.indexOf("\n\n");
    if (boundary === -1) break;

    const frame = remainder.slice(0, boundary);
    remainder = remainder.slice(boundary + 2);

    const event = parseSseFrame(frame);
    if (event) events.push(event);
  }

  return { events, remainder };
}

/**
 * Parse a single SSE frame (text between double newlines).
 * Returns { type, data } or null if not a valid event.
 */
function parseSseFrame(frame) {
  let type = "message";
  let data = "";

  for (const line of frame.split("\n")) {
    if (line.startsWith("event: ")) {
      type = line.slice(7);
    } else if (line.startsWith("data: ")) {
      data = line.slice(6);
    } else if (line.startsWith(":")) {
      // Comment line (heartbeat), skip
      continue;
    }
  }

  if (!data) return null;

  try {
    return { type, data: JSON.parse(data) };
  } catch {
    return { type, data };
  }
}
