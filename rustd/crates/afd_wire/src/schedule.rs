//! How a fleet's schedules are rendered to the people who own them.
//!
//! The dashboard and the CLI both read this surface, which is why the shape
//! lives here rather than beside the handler that fills it.

use std::borrow::Cow;

use serde::Serialize;

/// One schedule, as the tenant surface renders it.
#[derive(Debug, Clone, Serialize)]
pub struct View<'s> {
    /// Its identity.
    #[serde(borrow)]
    pub schedule_id: Cow<'s, str>,
    /// The fleet it wakes.
    #[serde(borrow)]
    pub fleet_id: Cow<'s, str>,
    /// The expression it fires on.
    #[serde(borrow)]
    pub cron: Cow<'s, str>,
    /// The zone that expression is read in.
    #[serde(borrow)]
    pub timezone: Cow<'s, str>,
    /// What the fleet is asked to do.
    #[serde(borrow)]
    pub message: Cow<'s, str>,
    /// What the operator wants it to be doing.
    #[serde(borrow)]
    pub status: Cow<'s, str>,
    /// How far the external scheduler has been brought in line.
    ///
    /// Rendered rather than hidden: a schedule that saved and did not register
    /// is the one state a person needs to see, and a view that showed only the
    /// intent would report a schedule as live when it fires nowhere.
    #[serde(borrow)]
    pub sync: Cow<'s, str>,
    /// Why the last push failed, when one did.
    #[serde(borrow)]
    pub last_error: Option<Cow<'s, str>>,
    /// When it was created.
    pub created_at: i64,
    /// When it was last changed.
    pub updated_at: i64,
}

/// A page of schedules.
#[derive(Debug, Clone, Serialize)]
pub struct Page<'s> {
    /// The fleet's schedules, oldest first.
    #[serde(borrow)]
    pub schedules: Vec<View<'s>>,
}
