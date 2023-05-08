use serenity::prelude::TypeMapKey;
use tracing::warn;

use crate::vc::enter_vc;
use crate::{CommandResult, Context};

crate::commands!(pause, resume, r#loop);

#[poise::command(slash_command, category = "Controls")]
/// Pause the current track
async fn pause(ctx: Context<'_>) -> CommandResult {
    enter_vc(ctx, false, |handler, c| async move {
        if let Err(e) = handler.lock().await.queue().pause() {
            warn!(?e, "failed to pause");
        }

        c.say("Paused").await?;

        Ok(())
    })
    .await
}

// TODO say which song we resumed
#[poise::command(slash_command, category = "Controls")]
/// Resume the current track
async fn resume(ctx: Context<'_>) -> CommandResult {
    enter_vc(ctx, false, |handler, c| async move {
        if let Err(e) = handler.lock().await.queue().resume() {
            warn!(?e, "failed to resume");
        }

        c.say("Resumed").await?;

        Ok(())
    })
    .await
}

pub enum LoopState {
    Disabled,
    Enabled,
}

impl TypeMapKey for LoopState {
    type Value = LoopState;
}

#[poise::command(slash_command, category = "Controls")]
/// Toggle loop mode for the current track
async fn r#loop(ctx: Context<'_>) -> CommandResult {
    enter_vc(ctx, false, |handler, c| async move {
        let lock = handler.lock().await;
        let current = lock.queue().current();
        drop(lock);
        let Some(current) = current else {
            c.say("No track is currently playing").await?;
            return Ok(());
        };

        let map = current.typemap().write().await;
        
        let new_state = match map.get() {
            Some(LoopState::Disabled) => LoopState::Enabled,
            Some(LoopState::Enabled) => LoopState::Disabled,
            None => LoopState::Enabled,
        };

        map.insert(new_state);

        drop(map);

        match new_state {
            LoopState::Disabled => {
                current.disable_loop()?;
                c.say("Looping disabled").await?;
            }
            LoopState::Enabled => {
                current.enable_loop()?;
                c.say("Looping enabled").await?;
            }
        }
        
    }).await
}
