//! The principals and verdicts the access suites build, spelled once.
//!
//! Every access suite asks the same resolver about the same kinds of caller:
//! a browser session naming a subject, and a claim-bound credential naming a
//! tenant. These are the constructors for both and for the verdicts they get.

use afd_auth::principal::{Person, PersonCredential, Principal, Subject};
use afd_auth::scope::ScopeSet;
use afd_core::id::Uuid7;
use afd_tenant::workspace::access::{Access, Grant, Role};

/// A stored or minted identifier, parsed.
pub(crate) fn id(value: &str) -> Uuid7 {
    Uuid7::parse(value).expect("the fixture identifier is UUIDv7")
}

/// A person proven by `credential`, claiming `tenant` as `subject`.
pub(crate) fn person(
    credential: PersonCredential,
    tenant: &str,
    subject: &str,
    scopes: ScopeSet,
) -> Principal {
    Principal::Person(Person::new(
        credential,
        id(tenant),
        Subject::new(subject).expect("the fixture subject is not blank"),
        scopes,
    ))
}

/// A browser session for `subject`, whose token claims `tenant`.
pub(crate) fn session(tenant: &str, subject: &str) -> Principal {
    person(
        PersonCredential::SessionToken {
            workspace_scope: None,
        },
        tenant,
        subject,
        ScopeSet::EMPTY,
    )
}

/// The verdict for `tenant`'s workspace held with `role`.
pub(crate) fn held(tenant: &str, role: Role) -> Access {
    Access {
        tenant: id(tenant),
        grant: Grant::Membership(role),
    }
}
