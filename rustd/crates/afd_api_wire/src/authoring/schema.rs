//! The draft patch request's published schema.
//!
//! [`DraftPatchRequest`] is generic so the daemon reads its edits straight
//! into the shared validator's types, and a derive on it cannot carry the batch
//! sizes that validator enforces: utoipa takes only literal numbers there. The
//! request is therefore described by the twin below and narrowed by
//! `afd_fleet_runtime`, which reads the same constants its validator does.

use std::borrow::Cow;

use afd_fleet_runtime::definition::{ConfigOp, InstructionEdit, narrow_edit_batch};
use utoipa::openapi::RefOr;
use utoipa::openapi::schema::Schema;
use utoipa::{PartialSchema, ToSchema};

use super::DraftPatchRequest;

/// One complete idle drawer mutation, protected by If-Match.
#[derive(utoipa::ToSchema)]
#[schema(as = DraftPatchRequest)]
#[serde(deny_unknown_fields)]
#[expect(
    dead_code,
    reason = "a schema description: utoipa reads the fields, and nothing builds one"
)]
struct PatchShape {
    /// Instance slug selected independently of the recipe and display title.
    #[schema(nullable = false)]
    instance_name: Option<String>,
    /// Human display title; 1..100 characters.
    #[schema(nullable = false)]
    title: Option<String>,
    /// Explicit selection for future messages.
    #[schema(nullable = false)]
    authoring_model_ref: Option<String>,
    /// Runtime model reference in the native Definition.
    #[schema(nullable = false)]
    runtime_model_ref: Option<String>,
    /// Supported add/replace/remove configuration operations.
    #[serde(default)]
    config_ops: Vec<ConfigOp>,
    /// Non-overlapping instruction edits.
    #[serde(default)]
    instruction_edits: Vec<InstructionEdit>,
}

impl<C, I> PartialSchema for DraftPatchRequest<C, I> {
    fn schema() -> RefOr<Schema> {
        let mut schema = PatchShape::schema();
        narrow_edit_batch::<PatchShape>(&mut schema);
        schema
    }
}

impl<C, I> ToSchema for DraftPatchRequest<C, I> {
    fn name() -> Cow<'static, str> {
        PatchShape::name()
    }

    fn schemas(schemas: &mut Vec<(String, RefOr<Schema>)>) {
        PatchShape::schemas(schemas);
    }
}
