//! The access request's validated alternatives on top of its derived fields.
//!
//! The runtime validator treats an optional null as absent. Keep that behavior
//! for older clients while requiring exactly one non-null target.
use std::borrow::Cow;

use af_wire::access_verb::AccessRequest;
use utoipa::openapi::RefOr;
use utoipa::openapi::schema::{AllOfBuilder, ObjectBuilder, OneOfBuilder, Schema, Type};
use utoipa::{PartialSchema, ToSchema};

#[derive(Debug)]
pub(super) struct AccessRequestSchema;

impl PartialSchema for AccessRequestSchema {
    fn schema() -> RefOr<Schema> {
        let absent = || ObjectBuilder::new().schema_type(Type::Null);
        let credential = ObjectBuilder::new()
            .property(
                "name",
                ObjectBuilder::new()
                    .schema_type(Type::String)
                    .min_length(Some(1)),
            )
            .required("name")
            .property("host", absent())
            .property("evidence", absent());
        let host = ObjectBuilder::new()
            .property("host", ObjectBuilder::new().schema_type(Type::String))
            .required("host")
            .property("name", absent())
            .property("scope", absent());
        AllOfBuilder::new()
            .item(AccessRequest::schema())
            .item(OneOfBuilder::new().item(credential).item(host))
            .description(Some("Provide one credential name or one host. Only host requests accept redirect evidence, and its to_host must match the requested host."))
            .into()
    }
}

impl ToSchema for AccessRequestSchema {
    fn name() -> Cow<'static, str> {
        AccessRequest::name()
    }

    fn schemas(schemas: &mut Vec<(String, RefOr<Schema>)>) {
        AccessRequest::schemas(schemas);
    }
}
