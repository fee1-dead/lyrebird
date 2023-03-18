// TODO remove this once WS is ready
#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use color_eyre::eyre::ContextCompat;
use serde::{Deserialize, Serialize};
use songbird::Call;

use crate::CommandResult;

/// A message that is sent to the server to control musicbot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Stops the current song and clears the queue.
    ClearAll,
    /// Pause current playback
    Pause,
    /// Resume current playback
    Resume,
    /// Skip the current song.
    Skip,
    /// Seek to a position in the current song.
    Seek(Duration),
}

async fn handle(call: Arc<tokio::sync::Mutex<Call>>, message: ClientMessage) -> CommandResult {
    use ClientMessage::*;
    match message {
        ClearAll => call.lock().await.queue().stop(),
        Pause => call.lock().await.queue().pause()?,
        Resume => call.lock().await.queue().resume()?,
        Skip => call.lock().await.queue().skip()?,
        Seek(duration) => {
            call.lock()
                .await
                .queue()
                .current()
                .context("no current song")?
                .seek(duration)
                .result_async()
                .await?;
        }
    }
    Ok(())
}

pub struct WsServer {}
