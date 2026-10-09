//! A resolver answering from a table, so a suite drives egress resolution,
//! the guarded client's and the supervisor's kernel set alike, without DNS.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::net::IpAddr;
use std::sync::{Arc, Mutex, PoisonError};

use crate::resolve::Resolve;

/// What a host outside the table answers with.
const NO_SUCH_HOST: &str = "no such host";

/// The answers each host gives, in turn.
type Answers = HashMap<String, VecDeque<Vec<IpAddr>>>;

/// A resolver whose every answer is in its table.
///
/// A host outside it does not resolve. A host with several answers gives them
/// in turn, then keeps giving the last, as a name a DNS change moved. Clones
/// share one table.
#[derive(Debug, Clone, Default)]
pub struct FakeResolver(Arc<Mutex<Answers>>);

impl FakeResolver {
    /// A resolver answering each host in `table` with its addresses.
    #[must_use]
    pub fn answering(table: &[(&str, &[IpAddr])]) -> Self {
        Self::moving(
            &table
                .iter()
                .map(|&(host, addresses)| (host, vec![addresses]))
                .collect::<Vec<_>>(),
        )
    }

    /// A resolver answering each host in `table` with each of its answers in
    /// turn.
    #[must_use]
    pub fn moving(table: &[(&str, Vec<&[IpAddr]>)]) -> Self {
        let answers = table
            .iter()
            .map(|(host, answers)| {
                let answers = answers.iter().map(|answer| answer.to_vec()).collect();
                ((*host).to_owned(), answers)
            })
            .collect();
        Self(Arc::new(Mutex::new(answers)))
    }
}

#[async_trait::async_trait]
impl Resolve for FakeResolver {
    async fn resolve(&self, host: &str) -> io::Result<Vec<IpAddr>> {
        let mut table = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let answers = table
            .get_mut(host)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, NO_SUCH_HOST))?;
        let answer = if answers.len() > 1 {
            answers.pop_front()
        } else {
            answers.front().cloned()
        };
        Ok(answer.unwrap_or_default())
    }
}
