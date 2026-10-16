//! The bounds the Definition's published schema carries beyond what the derive writes.
//!
//! # Why this is code and not `#[schema(...)]`
//!
//! utoipa 5.5 reads `max_length`, `max_items`, `minimum` and the rest as literal
//! tokens only; a constant in that position does not compile. Every bound the
//! parser enforces is a named constant, so writing the number a second time in
//! an attribute would be a copy that drifts the day the constant moves. Each
//! domain module instead lists its bounds against the SAME constants its garde
//! attributes read, and this module applies them to the derived schema.
//!
//! The derive still owns the shape: property names, types, requiredness,
//! nullability and descriptions. This layer only narrows.
//!
//! # Closed objects
//!
//! The parser refuses every key it does not know, but the raw types that
//! collect leftovers in a flattened `extra` map, and the variants of a tagged
//! enum, give the derive no way to say so. [`Published::closed`] does.

use std::borrow::Cow;

use utoipa::openapi::schema::{AdditionalProperties, ArrayItems, Schema};
use utoipa::openapi::{Object, RefOr};

/// The named schemas a `ToSchema::schemas` walk collects.
pub(crate) type Components = Vec<(String, RefOr<Schema>)>;

/// A string's length in characters, and the pattern it must match.
///
/// A garde bound counted in BYTES is published as the same number of
/// characters: no string over the byte cap has fewer characters than it, so
/// the published bound is one the parser never contradicts.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Text {
    /// The fewest characters. Zero publishes no minimum.
    pub(crate) min: usize,
    /// The most characters.
    pub(crate) max: usize,
    /// A regular expression the whole value must match.
    pub(crate) pattern: Option<&'static str>,
}

impl Text {
    /// A length range with no pattern.
    pub(crate) const fn chars(min: usize, max: usize) -> Self {
        Self {
            min,
            max,
            pattern: None,
        }
    }
}

/// One bound on one property.
#[derive(Clone)]
pub(crate) enum Bound {
    /// A string.
    Text(Text),
    /// A list's length, and the bound each entry carries.
    List {
        /// The fewest entries. Zero publishes no minimum.
        min: usize,
        /// The most entries.
        max: usize,
        /// The bound on each string entry, when the entries are strings.
        entry: Option<Text>,
    },
    /// A number's inclusive range.
    Range {
        /// The smallest accepted value.
        min: utoipa::Number,
        /// The largest accepted value.
        max: utoipa::Number,
    },
}

/// The bounds one published component carries.
pub(crate) struct Published {
    /// The component, by the name its derive publishes it under.
    pub(crate) component: fn() -> Cow<'static, str>,
    /// Whether every object in the component refuses keys it does not list.
    pub(crate) closed: bool,
    /// Each property's bound, by its published key.
    pub(crate) bounds: Vec<(&'static str, Bound)>,
}

/// Narrows every collected schema that `published` names.
pub(crate) fn apply(components: &mut Components, published: &[Published]) {
    for entry in published {
        let name = (entry.component)();
        components
            .iter_mut()
            .filter(|(found, _)| *found == name)
            .for_each(|(_, schema)| narrow(schema, entry));
    }
}

/// Narrows one schema: the object itself, or each variant of a `oneOf`.
pub(crate) fn narrow(schema: &mut RefOr<Schema>, published: &Published) {
    for object in objects(schema) {
        if published.closed {
            object.additional_properties = Some(Box::new(AdditionalProperties::FreeForm(false)));
        }
        for (key, bound) in &published.bounds {
            if let Some(RefOr::T(property)) = object.properties.get_mut(*key) {
                bound.apply(property);
            }
        }
    }
}

/// The object schemas a component is made of.
fn objects(schema: &mut RefOr<Schema>) -> Vec<&mut Object> {
    match schema {
        RefOr::T(Schema::Object(object)) => vec![object],
        RefOr::T(Schema::OneOf(one_of)) => one_of
            .items
            .iter_mut()
            .filter_map(|variant| match variant {
                RefOr::T(Schema::Object(object)) => Some(object),
                _other => None,
            })
            .collect(),
        _other => Vec::new(),
    }
}

impl Text {
    /// Writes this bound onto a string schema.
    fn apply(self, object: &mut Object) {
        object.min_length = (self.min > 0).then_some(self.min);
        object.max_length = Some(self.max);
        object.pattern = self.pattern.map(str::to_owned);
    }
}

impl Bound {
    /// Writes this bound onto the property schema it names.
    fn apply(&self, property: &mut Schema) {
        match (self, property) {
            (Self::Text(text), Schema::Object(object)) => text.apply(object),
            (Self::Range { min, max }, Schema::Object(object)) => {
                object.minimum = Some(min.clone());
                object.maximum = Some(max.clone());
            }
            (&Self::List { min, max, entry }, Schema::Array(array)) => {
                array.min_items = (min > 0).then_some(min);
                array.max_items = Some(max);
                if let (Some(text), ArrayItems::RefOrSchema(items)) = (entry, &mut array.items)
                    && let RefOr::T(Schema::Object(item)) = items.as_mut()
                {
                    text.apply(item);
                }
            }
            _mismatch => {}
        }
    }
}

#[cfg(test)]
mod tests;
