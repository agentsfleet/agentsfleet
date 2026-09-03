//! The events answer's schema, written out rather than derived.
//!
//! Every other shape in [`super`] derives one. This enum cannot: the derive
//! publishes `oneOf`, and the echo is a free-form object that ALSO matches
//! `{"ignored": …}`, so a strict client would refuse every ignored answer as
//! ambiguous. `anyOf` is the claim that is true, and no utoipa attribute asks
//! for it, so both impls are spelled here.

use std::borrow::Cow;

use super::{EchoAnswer, EventsAnswer, Ignored};

impl utoipa::PartialSchema for EventsAnswer<'_> {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::ToSchema as _;
        utoipa::openapi::RefOr::T(utoipa::openapi::schema::Schema::AnyOf(
            utoipa::openapi::schema::AnyOfBuilder::new()
                .item(utoipa::openapi::Ref::from_schema_name(EchoAnswer::name()))
                .item(utoipa::openapi::Ref::from_schema_name(Ignored::name()))
                .description(Some(
                    "A handshake echoed under the provider's own field name, or a \
                     delivery acknowledged and not acted on, with the reason",
                ))
                .build(),
        ))
    }
}

impl utoipa::ToSchema for EventsAnswer<'_> {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("EventsAnswer")
    }

    fn schemas(
        schemas: &mut Vec<(
            String,
            utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
        )>,
    ) {
        use utoipa::PartialSchema as _;
        schemas.push((EchoAnswer::name().into_owned(), EchoAnswer::schema()));
        schemas.push((Ignored::name().into_owned(), Ignored::schema()));
        EchoAnswer::schemas(schemas);
        Ignored::schemas(schemas);
    }
}
