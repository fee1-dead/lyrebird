use tracing::warn;

use crate::vc::enter_vc;
use crate::{CommandResult, Context};

crate::commands!(pause, resume);

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
