//! Backpressure regulator for the `EngineInput` channel.
//!
//! The [`BackpressureRegulator`] guarantees that the bounded channel
//! to the engine never exceeds its capacity. In `Conservative` mode,
//! intermediate SSE text chunks are dropped before the producer blocks.
//! Structural events (`ToolCallStart`, `ToolCallDone`) are never dropped.
//!
//! Refs: I-Shell-Backpressure-NoOverflow

use brioche_core::{EngineInput, StreamEvent};
use tokio::sync::mpsc;

/// Drop policy when the engine channel is under pressure.
///
/// Refs: SPECS.md §Book III-A Ch 2
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropPolicy {
    /// Drop intermediate text chunks, keep structural boundaries.
    Conservative,
    /// Never drop — blocks the SSE producer.
    Strict,
}

/// Regulates flow into the engine's `EngineInput` channel.
///
/// # Example
///
/// ```
/// use brioche_core::EngineInput;
/// use brioche_shell_runtime::{BackpressureRegulator, DropPolicy};
///
/// # async fn example() {
/// let (tx, mut rx) = BackpressureRegulator::new(128, DropPolicy::Conservative);
/// tx.send(EngineInput::UserMessage("hello".into()))
///     .await
///     .unwrap();
/// # }
/// ```
///
/// Refs: I-Shell-Backpressure-NoOverflow
#[derive(Clone)]
pub struct BackpressureRegulator {
    tx: mpsc::Sender<EngineInput>,
    capacity: usize,
    drop_policy: DropPolicy,
}

impl BackpressureRegulator {
    /// Create a new regulator with the given channel capacity and drop policy.
    ///
    /// Returns the regulator handle and the receiver end that should be
    /// wired into the engine thread's input loop.
    /// Refs: SPECS.md §Book III-A
    pub fn new(capacity: usize, drop_policy: DropPolicy) -> (Self, mpsc::Receiver<EngineInput>) {
        let (tx, rx) = mpsc::channel(capacity);
        let regulator = Self {
            tx,
            capacity,
            drop_policy,
        };
        (regulator, rx)
    }

    /// Send an input into the engine channel.
    ///
    /// - In `Conservative` mode: attempts a non-blocking send first.
    ///   If the channel is full and the input is an intermediate
    ///   `LlmStream::TextChunk`, it is silently dropped. Structural
    ///   events are never dropped.
    /// - In `Strict` mode: waits for capacity unconditionally.
    ///
    /// Returns `Err` only if the receiver has been dropped.
    ///
    /// # Cancel safety
    /// In `Conservative` mode, the non-blocking path is cancellation-safe.
    /// In `Strict` mode, this future holds no locks across await points;
    /// dropping it before completion only fails to enqueue the input.
    pub async fn send(
        &self,
        input: EngineInput,
    ) -> Result<(), mpsc::error::SendError<EngineInput>> {
        match self.drop_policy {
            DropPolicy::Conservative => {
                // Try non-blocking first.
                match self.tx.try_send(input) {
                    Ok(()) => Ok(()),
                    Err(mpsc::error::TrySendError::Full(input)) => {
                        // Under pressure: drop intermediate text chunks only.
                        if let EngineInput::LlmStream(StreamEvent::TextChunk { .. }) = &input {
                            Ok(())
                        } else {
                            // Structural event: block until capacity.
                            self.tx.send(input).await
                        }
                    }
                    Err(mpsc::error::TrySendError::Closed(input)) => {
                        Err(mpsc::error::SendError(input))
                    }
                }
            }
            DropPolicy::Strict => self.tx.send(input).await,
        }
    }

    /// Returns the configured capacity of the channel.
    /// Refs: SPECS.md §Book III-A
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}
