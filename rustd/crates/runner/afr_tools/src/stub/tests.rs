//! A stub offers the schema derived for its argument type, never JSON written
//! beside it.

use guffbolt::Describe as _;
use serde_json::json;

use super::{NoArguments, Stub};
use crate::catalog::UPDATE_PLAN;
use guffbolt::tools::Schema;

#[test]
fn test_stub_schema_is_derived() {
    let stub = Stub::new(&UPDATE_PLAN);
    let derived = Schema::of::<NoArguments>(UPDATE_PLAN.name());

    assert_eq!(stub.spec(), derived.spec(UPDATE_PLAN.name()));

    // The derived shape is the one every provider reads: a flat object that
    // takes nothing and refuses whatever key a model invents.
    assert_eq!(
        stub.spec().parameters,
        &json!({
            "type": "object",
            "additionalProperties": false,
            "description": "Takes no arguments.",
        })
    );
}
