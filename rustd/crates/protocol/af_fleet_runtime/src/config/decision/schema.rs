//! Role-specific published shapes share native answer keys and text ceilings.

use std::borrow::Cow;

use utoipa::{
    PartialSchema, ToSchema,
    openapi::{
        RefOr,
        schema::{AdditionalProperties, ObjectBuilder, Schema, Type},
    },
};

use super::{
    ASSOCIATION_CRITERIA, INTENT_OPTIONS, MAX_QUESTION_TEXT_BYTES, NEXT_ACTION_OPTIONS,
    NOUL_CRITERIA,
};

pub(super) const TEXT_PATTERN: &str = r"^[^\x00]*[^\s\x00][^\x00]*$";

/// The intent question with its six required criteria.
#[derive(Debug)]
pub(crate) enum IntentQuestion {}
/// The association question; native task aliases are supplied at dispatch.
#[derive(Debug)]
pub(crate) enum AssociationQuestion {}
/// A probability-of-yes question with both criteria.
#[derive(Debug)]
pub(crate) enum NoulQuestion {}
/// The next-action question with four required criteria.
#[derive(Debug)]
pub(crate) enum NextActionQuestion {}
/// Both required probability-of-yes criteria for a completion statement.
#[derive(Debug)]
pub(crate) enum NoulCriteria {}

impl PartialSchema for IntentQuestion {
    fn schema() -> RefOr<Schema> {
        question(&INTENT_OPTIONS)
    }
}
impl ToSchema for IntentQuestion {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("DefinitionIntentQuestion")
    }
}
impl PartialSchema for AssociationQuestion {
    fn schema() -> RefOr<Schema> {
        question(&ASSOCIATION_CRITERIA)
    }
}
impl ToSchema for AssociationQuestion {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("DefinitionAssociationQuestion")
    }
}
impl PartialSchema for NoulQuestion {
    fn schema() -> RefOr<Schema> {
        question(&NOUL_CRITERIA)
    }
}
impl ToSchema for NoulQuestion {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("DefinitionNoulQuestion")
    }
}
impl PartialSchema for NextActionQuestion {
    fn schema() -> RefOr<Schema> {
        question(&NEXT_ACTION_OPTIONS)
    }
}
impl ToSchema for NextActionQuestion {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("DefinitionNextActionQuestion")
    }
}
impl PartialSchema for NoulCriteria {
    fn schema() -> RefOr<Schema> {
        criteria(&NOUL_CRITERIA).into()
    }
}
impl ToSchema for NoulCriteria {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("DefinitionNoulCriteria")
    }
}

fn question(keys: &[&str]) -> RefOr<Schema> {
    ObjectBuilder::new().schema_type(Type::Object)
        .additional_properties(Some(AdditionalProperties::FreeForm(false)))
        .property("question", text()).required("question")
        .property("criteria", criteria(keys)).required("criteria")
        .description(Some("Owner-reviewed wording and exact native role criteria; complete authored JSON is at most 1 KiB."))
        .into()
}

fn criteria(keys: &[&str]) -> ObjectBuilder {
    let mut schema = ObjectBuilder::new()
        .schema_type(Type::Object)
        .additional_properties(Some(AdditionalProperties::FreeForm(false)));
    for key in keys {
        schema = schema.property(*key, text()).required(*key);
    }
    schema
}

fn text() -> ObjectBuilder {
    ObjectBuilder::new().schema_type(Type::String).min_length(Some(1))
        .max_length(Some(MAX_QUESTION_TEXT_BYTES))
        .pattern(Some(TEXT_PATTERN))
        .description(Some("Nonblank, NUL-free text, at most 512 UTF-8 bytes. The server also checks the encoded byte length."))
}
