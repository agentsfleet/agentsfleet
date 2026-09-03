//! The response bodies that are not JSON, described for the document.
//!
//! # Why a type and not `[u8]` at the annotation
//!
//! utoipa reads a byte slice as an ARRAY OF INTEGERS: it picks the octet-stream
//! media type from it, and then describes the schema as `[0, 255, ...]`, which
//! a generated client parses as JSON and fails on the first byte of a tar. The
//! document needs `type: string, format: binary`, the shape every generator
//! turns into "hand the caller the bytes", and the only way to say that in the
//! path macro is a type whose schema is overridden to it.
//!
//! # These describe the wire, not the handler
//!
//! Like [`super::path`], nothing here is constructed at runtime. The handler
//! that serves a tar writes the bytes it was given; this is what the document
//! says about them, kept in the substrate so a second route answering bytes
//! names the same shape rather than re-spelling it.

use utoipa::ToSchema;

/// A body of raw bytes, read whole and never parsed.
#[derive(Debug, ToSchema)]
#[schema(value_type = String, format = Binary)]
pub struct Binary(pub Vec<u8>);
