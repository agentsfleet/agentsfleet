use tokio::task::JoinSet;

use crate::Result;

pub(crate) async fn completed(workers: &mut JoinSet<Result<()>>) -> Result<()> {
    if let Some(joined) = workers.join_next().await {
        let outcome = joined?;
        outcome?;
    }
    Ok(())
}
