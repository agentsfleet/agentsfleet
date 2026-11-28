import { Buffer } from "node:buffer";
import { Effect } from "effect";
import { ValidationError } from "../../errors/index.ts";

const MAX_BYTES = 5 * 1024 * 1024; // Messages provider limit per image.
const HELP = "Use a PNG, JPEG, GIF or WebP file of at most 5 MiB.";

export type MessageImage = { readonly media_type: string; readonly data: string };

export function messageImage(path: string): Effect.Effect<MessageImage, ValidationError> {
  return Effect.tryPromise({
    try: async () => {
      const file = Bun.file(path);
      // Slice also bounds the read if the source grows before it opens.
      return Buffer.from(await file.slice(0, MAX_BYTES + 1).arrayBuffer());
    },
    catch: () => new ValidationError({ detail: "Unable to read the image file", suggestion: HELP }),
  }).pipe(Effect.flatMap(bytes => {
    const type = mediaType(bytes);
    if (bytes.length > MAX_BYTES || type === undefined) {
      return Effect.fail(new ValidationError({ detail: "Invalid image file or size", suggestion: HELP }));
    }
    return Effect.succeed({ media_type: type, data: bytes.toString("base64") });
  }));
}

function mediaType(bytes: Buffer): string | undefined {
  if (bytes.subarray(0, 8).toString("hex") === "89504e470d0a1a0a") return "image/png";
  if (bytes.subarray(0, 3).toString("hex") === "ffd8ff") return "image/jpeg";
  if (bytes.subarray(0, 4).toString("ascii") === "GIF8") return "image/gif";
  if (bytes.subarray(0, 4).toString("ascii") === "RIFF" && bytes.subarray(8, 12).toString("ascii") === "WEBP") return "image/webp";
  return undefined;
}
