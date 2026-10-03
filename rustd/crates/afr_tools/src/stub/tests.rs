use super::{NoArguments, Stub};
use crate::catalog::FILE_READ;
use crate::runtime::Tool as _;
use crate::schema::Schema;

#[test]
fn test_stub_schema_is_derived() {
    let stub = Stub::new(&FILE_READ);

    assert_eq!(
        stub.schema().parameters(),
        Schema::of::<NoArguments>(FILE_READ.name()).parameters()
    );
    assert_eq!(stub.schema().description(), FILE_READ.name());
    assert_eq!(stub.schema().parameters()["additionalProperties"], false);
}
