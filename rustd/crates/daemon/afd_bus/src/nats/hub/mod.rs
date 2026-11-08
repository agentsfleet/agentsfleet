//! One native subscription per subject, fanned out to every local reader.
//!
//! The first reader of a subject subscribes natively and starts its pump; every
//! later reader gets a broadcast receiver on the same channel. The last reader's
//! drop aborts the pump, and dropping the native subscriber unsubscribes it. A
//! gap reaches every reader of a subject at once, so a backfill closes it.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use async_nats::Subscriber;
use futures_util::StreamExt as _;
use tokio::{sync::broadcast, task::JoinHandle};

use crate::{Message, Received, Result, Subject, Subscription, error::ErrorKind};

/// The header that turns an empty message into a gap.
pub(super) const GAP_HEADER: &str = "Af-Gap";
/// The header's only value.
pub(super) const GAP_VALUE: &str = "1";

/// What the pump hands each reader.
#[derive(Debug, Clone)]
pub(super) enum Delivery {
    /// One payload, shared by every reader of the subject.
    Message(Arc<Message>),
    /// Frames on this subject may be missing.
    Gap,
}

/// Read one native message: the gap header wins over any payload, and a
/// payload that is not UTF-8 was not published by this system.
#[must_use]
pub(super) fn classify(subject: &Subject, native: &async_nats::Message) -> Option<Delivery> {
    let marked = native
        .headers
        .as_ref()
        .and_then(|headers| headers.get(GAP_HEADER))
        .is_some_and(|value| value.as_str() == GAP_VALUE);
    if marked {
        return Some(Delivery::Gap);
    }
    let payload = String::from_utf8(native.payload.to_vec()).ok()?;
    Some(Delivery::Message(Arc::new(Message {
        subject: subject.clone(),
        payload: Arc::new(payload),
    })))
}

#[derive(Debug)]
struct Channel {
    sender: broadcast::Sender<Delivery>,
    readers: usize,
    generation: u64,
    pump: Option<JoinHandle<()>>,
}

#[derive(Debug, Default)]
struct State {
    channels: HashMap<String, Channel>,
    next_generation: u64,
}

/// Every subject this process reads, and its readers.
#[derive(Debug)]
pub(super) struct Channels {
    /// Messages a slow reader may fall behind before it is told how many it lost.
    capacity: usize,
    state: Mutex<State>,
}

/// One reader's place on a channel; dropping it releases the reader.
#[derive(Debug)]
pub(super) struct Reader {
    channels: Arc<Channels>,
    physical: String,
    generation: u64,
    receiver: broadcast::Receiver<Delivery>,
}

/// A new channel's sender, for its pump.
#[derive(Debug)]
pub(super) struct Opened {
    /// The reader that asked.
    pub(super) reader: Reader,
    /// The pump's sender and generation; `None` when the subject already had one.
    pub(super) pump: Option<(broadcast::Sender<Delivery>, u64)>,
}

impl Channels {
    /// No subjects yet; each new one buffers `capacity` messages per reader.
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            capacity,
            state: Mutex::default(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // Each critical section leaves the map whole, so a panic elsewhere
        // cannot leave it half-written.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn reader(self: &Arc<Self>, physical: &str, channel: &Channel) -> Reader {
        Reader {
            channels: Arc::clone(self),
            physical: physical.to_owned(),
            generation: channel.generation,
            receiver: channel.sender.subscribe(),
        }
    }

    /// Join a subject, or open it when this is its first reader. The opener
    /// receives the sender its pump publishes through.
    #[must_use]
    pub(super) fn open(self: &Arc<Self>, physical: &str) -> Opened {
        let mut state = self.lock();
        if let Some(channel) = state.channels.get_mut(physical) {
            channel.readers += 1;
            let reader = self.reader(physical, channel);
            return Opened { reader, pump: None };
        }
        state.next_generation += 1;
        let generation = state.next_generation;
        let (sender, _) = broadcast::channel(self.capacity);
        let channel = Channel {
            sender: sender.clone(),
            readers: 1,
            generation,
            pump: None,
        };
        let reader = self.reader(physical, &channel);
        state.channels.insert(physical.to_owned(), channel);
        Opened {
            reader,
            pump: Some((sender, generation)),
        }
    }

    /// Record the pump so the last reader can stop it. A channel whose
    /// readers already left stops the pump at once.
    pub(super) fn started(&self, physical: &str, generation: u64, pump: JoinHandle<()>) {
        let mut state = self.lock();
        match state.channels.get_mut(physical) {
            Some(channel) if channel.generation == generation => channel.pump = Some(pump),
            _ => pump.abort(),
        }
    }

    /// The native side ended: readers see the channel close.
    pub(super) fn ended(&self, physical: &str, generation: u64) {
        let mut state = self.lock();
        if state
            .channels
            .get(physical)
            .is_some_and(|channel| channel.generation == generation)
        {
            state.channels.remove(physical);
        }
    }

    fn release(&self, physical: &str, generation: u64) {
        let mut state = self.lock();
        let Some(channel) = state.channels.get_mut(physical) else {
            return;
        };
        if channel.generation != generation {
            return;
        }
        channel.readers -= 1;
        if channel.readers == 0
            && let Some(channel) = state.channels.remove(physical)
            && let Some(pump) = channel.pump
        {
            pump.abort();
        }
    }

    /// Send every reader of every subject a gap; returns how many received it.
    pub(super) fn gap_all(&self) -> usize {
        let state = self.lock();
        state
            .channels
            .values()
            .map(|channel| channel.sender.send(Delivery::Gap).unwrap_or(0))
            .sum()
    }

    /// Stop every pump, so each reader sees its channel close, and hand the
    /// stopped tasks back for the caller to join.
    #[must_use]
    pub(super) fn close(&self) -> Vec<JoinHandle<()>> {
        let mut state = self.lock();
        state
            .channels
            .drain()
            .filter_map(|(_, channel)| channel.pump)
            .inspect(JoinHandle::abort)
            .collect()
    }

    /// Readers on one subject.
    #[cfg(test)]
    #[must_use]
    pub(super) fn readers(&self, physical: &str) -> usize {
        self.lock()
            .channels
            .get(physical)
            .map_or(0, |channel| channel.readers)
    }
}

impl Reader {
    /// Wait for the next delivery, or for the count of those missed.
    ///
    /// # Errors
    /// Returns `Closed` once the channel's pump has ended.
    pub(super) async fn recv(&mut self) -> Result<Received> {
        match self.receiver.recv().await {
            Ok(Delivery::Message(message)) => Ok(Received::Message(message)),
            Ok(Delivery::Gap) => Ok(Received::Gap),
            Err(broadcast::error::RecvError::Lagged(missed)) => Ok(Received::Lagged(missed)),
            Err(broadcast::error::RecvError::Closed) => Err(ErrorKind::Closed.into()),
        }
    }

    /// The bus-facing stream; it ends after the first error.
    #[must_use]
    pub(super) fn into_subscription(self) -> Subscription {
        Subscription::new(futures_util::stream::unfold(Some(self), async |state| {
            let mut reader = state?;
            match reader.recv().await {
                Ok(received) => Some((Ok(received), Some(reader))),
                Err(error) => Some((Err(error), None)),
            }
        }))
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        self.channels.release(&self.physical, self.generation);
    }
}

/// Forward one native subscription into its channel until either side ends.
pub(super) async fn pump(
    mut native: Subscriber,
    sender: broadcast::Sender<Delivery>,
    subject: Subject,
    channels: Arc<Channels>,
    physical: String,
    generation: u64,
) {
    while let Some(message) = native.next().await {
        let Some(delivery) = classify(&subject, &message) else {
            let subject_kind = subject.kind();
            let error_code = af_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
            tracing::warn!(event = "bus_message_not_utf8", subject_kind, error_code);
            continue;
        };
        // No receiver means every reader left between the message and now.
        let _ = sender.send(delivery);
    }
    channels.ended(&physical, generation);
}

#[cfg(test)]
mod tests;
