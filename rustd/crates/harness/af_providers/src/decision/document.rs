//! Preserve ambiguity before JSON object insertion replaces a repeated key.

use std::fmt;

use af_wire::decision::DecisionApi;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};

use crate::Result;

pub(super) struct Document {
    pub value: Value,
    pub ambiguous: bool,
    pub usage_ambiguous: bool,
    pub model_ambiguous: bool,
}

#[derive(Default)]
struct Ambiguity {
    any: bool,
    usage: bool,
    model: bool,
}

#[derive(Clone, Copy)]
enum Scope {
    Outer,
    Envelope,
    Usage,
    Other,
}

impl Scope {
    fn child(self, key: &str) -> Self {
        match (self, key) {
            (Self::Outer, "result") => Self::Envelope,
            (Self::Envelope, "usage") | (Self::Usage, _) => Self::Usage,
            _ => Self::Other,
        }
    }

    fn covers_usage(self, key: &str) -> bool {
        matches!(self, Self::Usage)
            || matches!(
                (self, key),
                (Self::Envelope, "usage") | (Self::Outer, "result")
            )
    }

    fn covers_model(self, key: &str) -> bool {
        matches!(
            (self, key),
            (Self::Envelope, "model") | (Self::Outer, "result")
        )
    }
}

pub(super) fn parse(bytes: &[u8], api: DecisionApi) -> Result<Document> {
    let mut ambiguity = Ambiguity::default();
    let scope = if api == DecisionApi::Cloudflare {
        Scope::Outer
    } else {
        Scope::Envelope
    };
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = Seed {
        ambiguity: &mut ambiguity,
        scope,
    }
    .deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(Document {
        value,
        ambiguous: ambiguity.any,
        usage_ambiguous: ambiguity.usage,
        model_ambiguous: ambiguity.model,
    })
}

struct Seed<'a> {
    ambiguity: &'a mut Ambiguity,
    scope: Scope,
}

impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Value;

    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Seed<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a bounded decision document")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        // The JSON decoder rejects nonfinite numbers before visiting them.
        Ok(Value::from(v))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element_seed(Seed {
            ambiguity: &mut *self.ambiguity,
            scope: self.scope,
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut values = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            let covers_usage = self.scope.covers_usage(&key);
            let covers_model = self.scope.covers_model(&key);
            let value = map.next_value_seed(Seed {
                ambiguity: &mut *self.ambiguity,
                scope: self.scope.child(&key),
            })?;
            if values.insert(key, value).is_some() {
                self.ambiguity.any = true;
                self.ambiguity.usage |= covers_usage;
                self.ambiguity.model |= covers_model;
            }
        }
        Ok(Value::Object(values))
    }
}
