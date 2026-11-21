# guffbolt-ai

The provider side of the [guffbolt](https://crates.io/crates/guffbolt) agent
harness.

It holds the types one model call reads and writes, and the `Model` trait a
provider implements:

- `Request`: what one step asks the model.
- `Chunk`: one piece of the streamed answer, ending with `Chunk::End`.
- `Message`, `Call` and `Replay`: the conversation a step continues.
- `ToolSpec`, `Image` and `ImageKind`: a tool as the model is offered it, and
  an image a call hands back.
- `Usage`: the tokens one step spent.

guffbolt depends on this crate and re-exports every type under its own path.
A provider crate implements `Model` with this crate alone. A loop drives it
with guffbolt.
