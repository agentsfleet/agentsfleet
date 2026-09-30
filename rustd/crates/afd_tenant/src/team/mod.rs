//! The people in an account: who is invited, who accepted, who may be removed.
//!
//! One store over the two halves of one question. An owner invites an address;
//! the person holding it accepts, which writes the membership the access check
//! reads; the owner may later remove them, but never the account's last owner.
//! Invites grant the whole account, and every invite grants the member role.

pub mod email;

mod accept;
mod invite;
mod member;

use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;

use crate::workspace::access::Role;

pub use self::email::Email;

/// How long an invite can be accepted: seven days.
pub const INVITE_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// Invites, acceptance and members, over one pool.
///
/// Cheap to clone: both members are handles.
#[derive(Debug, Clone)]
pub struct Team {
    database: Db,
    entropy: Entropy,
}

impl Team {
    /// A store over `database`, minting identifiers from `entropy`.
    #[must_use]
    pub const fn new(database: Db, entropy: Entropy) -> Self {
        Self { database, entropy }
    }
}

/// What a new invite needs.
#[derive(Debug, Clone, Copy)]
pub struct NewInvite<'a> {
    /// The account the invite opens.
    pub tenant: &'a Uuid7,
    /// The owner issuing it.
    pub inviter: &'a Uuid7,
    /// The address it is for.
    pub email: &'a Email,
}

/// One issued invite, as the account's owner sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    /// The invite's identifier, which its accept link names.
    pub id: Uuid7,
    /// The address it is for, lowercased.
    pub email: String,
    /// The role accepting it grants.
    pub role: Role,
    /// When it stops being acceptable.
    pub expires_at_ms: i64,
    /// When it was issued.
    pub created_at_ms: i64,
}

/// One invite waiting for an address, with the account it opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    /// The invite's identifier.
    pub id: String,
    /// The account it opens.
    pub tenant: String,
    /// What a person calls that account.
    pub owner_name: String,
    /// When it stops being acceptable.
    pub expires_at_ms: i64,
}

/// The person accepting an invite.
#[derive(Debug, Clone, Copy)]
pub struct Invitee<'a> {
    /// Their user row.
    pub user: &'a Uuid7,
    /// The address the identity provider verified for them, as stored.
    pub email: &'a str,
}

/// What an accept opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accepted {
    /// The account joined.
    pub tenant: Uuid7,
    /// Its workspaces, oldest first.
    pub workspaces: Vec<String>,
}

/// One member of an account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// Their user row.
    pub user: String,
    /// Their name, when the identity provider supplied one.
    pub display_name: Option<String>,
    /// Their address, as stored.
    pub email: String,
    /// Their role in the account.
    pub role: Role,
}

/// What a removal did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removal {
    /// The membership was removed.
    Removed,
    /// There was no such membership; removing it again changes nothing.
    Absent,
}
