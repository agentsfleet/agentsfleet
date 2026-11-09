//! Only an exact confirmation can turn a proposal into a database-issued session authority.

use super::super::{ConfirmedSession, Message, SessionOpening, cache::LeaseCache};
use super::allowed;
use crate::{
    Operation, Result,
    connection::Binding,
    error::{ErrorKind, within},
};
use af_wire::protocol::{StreamKind, Up};
use garde::Validate as _;
use tokio::sync::mpsc;

impl SessionOpening {
    /// Keep proposal exchange separate from the committed session's transport supervision.
    pub(in super::super) async fn confirm(mut self) -> Result<ConfirmedSession> {
        self.send_prepared().await?;
        self.receive_confirmation().await?;
        let authority = within(
            &self.context.native.stop,
            self.deadline,
            Operation::Opening,
            async {
                allowed(
                    self.gateway
                        .inner
                        .dependencies
                        .runners
                        .confirm_connection(self.prepared)
                        .await?,
                )
            },
        )
        .await?;
        let cache = within(
            &self.context.native.stop,
            self.deadline,
            Operation::Opening,
            LeaseCache::rebuild(
                self.gateway.inner.dependencies.leases.leases.pool(),
                authority.runner(),
            ),
        )
        .await?;
        let (attachments, receiver) = mpsc::channel(3);
        self.context.bind(Binding {
            runner: self.runner.clone(),
            authority: authority.clone(),
            attachments,
        })?;
        Ok(ConfirmedSession {
            gateway: self.gateway,
            context: self.context,
            runner: self.runner,
            hello: self.hello,
            authority,
            cache,
            attachments: receiver,
            io: self.io,
            permit: self.permit,
            deadline: self.deadline,
        })
    }

    /// The runner must remember the observed and proposed owners before acknowledging this message.
    async fn send_prepared(&mut self) -> Result<()> {
        let prepared = Message::Prepared {
            request_id: self.hello.request_id,
            connection_id: self.prepared.connection().as_str().into(),
            previous_connection_id: self.prepared.previous().map(|id| id.as_str().into()),
        };
        let frame = af_stream::encode(
            &prepared,
            &self.gateway.inner.output_budget,
            StreamKind::Control,
            &self.context.native.stop,
            self.deadline,
        )
        .await?;
        af_stream::write(
            &mut self.io.writer,
            &frame,
            &self.context.native.stop,
            self.deadline,
        )
        .await?;
        Ok(())
    }

    /// The correlation identifier and proposal must both match; arbitrary confirmations change nothing.
    async fn receive_confirmation(&mut self) -> Result<()> {
        let frame = within(
            &self.context.native.stop,
            self.deadline,
            Operation::Opening,
            async {
                self.io
                    .reader
                    .next()
                    .await?
                    .ok_or_else(|| ErrorKind::Closed.into())
            },
        )
        .await?;
        let message: Up<'_> = frame.decode()?;
        message.validate()?;
        match message {
            Up::Confirm {
                request_id,
                connection_id,
            } if request_id == self.hello.request_id
                && connection_id.as_ref() == self.prepared.connection().as_str() =>
            {
                Ok(())
            }
            _ => Err(ErrorKind::Protocol.into()),
        }
    }
}
