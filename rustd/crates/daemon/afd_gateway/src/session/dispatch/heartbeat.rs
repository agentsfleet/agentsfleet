//! A heartbeat carries the current owner check in its policy read and never holds for work.

use std::num::NonZeroU64;

use af_wire::{protocol::Answer, runner::HeartbeatRequest};
use afd_runner::heartbeat::holds;

use super::super::{Message, SessionServices, context::answer, opening::allowed};
use crate::{Operation, Result};

impl SessionServices {
    pub(in super::super) async fn heartbeat(
        &self,
        request_id: NonZeroU64,
        mut request: HeartbeatRequest<'_>,
    ) -> Result<()> {
        let held = garde::Unvalidated::new(std::mem::take(&mut request.holds)).validate()?;
        let listed = holds::fleets(&held);
        self.handle.liveness.received();
        let result = self
            .operation(Operation::Heartbeat, async {
                allowed(
                    self.gateway
                        .inner
                        .dependencies
                        .runners
                        .heartbeat_connected(
                            &self.authority,
                            &request,
                            Some(&held),
                            self.gateway.inner.dependencies.wall_clock.now(),
                        )
                        .await?,
                )
            })
            .await;
        match result {
            Ok(beat) => {
                self.handle.snapshot.lock().held = listed;
                self.apply_policy(&beat);
                self.send(&Message::Heartbeat {
                    request_id,
                    answer: Answer::Ok {
                        response: Self::policy(&beat)?,
                    },
                })
                .await?;
                self.refresh_cancellations().await?;
            }
            Err(error) => {
                let terminal = !error.retryable();
                let code = error.code().as_str();
                self.send(&Message::Heartbeat {
                    request_id,
                    answer: answer(Err(error)),
                })
                .await?;
                if terminal {
                    self.send(&Message::GoAway {
                        error_code: Some(code.into()),
                    })
                    .await?;
                    self.handle.stop.cancel();
                }
            }
        }
        Ok(())
    }
}
