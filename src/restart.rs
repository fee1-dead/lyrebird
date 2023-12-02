use std::env;
use std::mem::take;
use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

use crate::metadata::QueueableKey;
use crate::play::Queueable;
use crate::{CommandResult, Context};

crate::commands!(restart);

#[derive(Serialize, Deserialize, Debug)]
pub struct CallData {
    pub guild: NonZeroU64,
    pub channel: NonZeroU64,
    pub queue: Vec<Queueable>,
}

#[poise::command(prefix_command)]
/// Restart the bot
pub async fn restart(ctx: Context<'_>) -> CommandResult {
    let is_bot_owner = ctx.framework().options().owners.contains(&ctx.author().id);
    if !is_bot_owner {
        ctx.say("Can only be used by bot owner").await?;
        return Ok(());
    }

    if !env::vars().any(|(key, _)| key == "IS_RUN_BY_RUNNER") {
        ctx.say("Not being run by the runner, therefore restart will not work")
            .await?;
        return Ok(());
    }

    let songbird = songbird::get(ctx.serenity_context()).await.unwrap();
    let mut calls = Vec::new();
    for (guild, call) in songbird.iter() {
        let mut handler = call.lock().await;
        // first pause playback in the queue
        handler.queue().pause()?;

        let Some(ch) = handler.current_channel() else { continue; };

        let queue = handler.queue().modify_queue(|x| take(x));
        let mut data = Vec::with_capacity(queue.len());
        for x in &queue {
            x.stop()?;
            let q = x.typemap().read().await;
            let Some(q) = q.get::<QueueableKey>() else { continue; };
            data.push(q.clone());
        }
        handler.leave().await?;
        calls.push(CallData {
            guild: guild.0,
            channel: ch.0,
            queue: data,
        })
    }

    let tmp = tempfile::NamedTempFile::new()?;
    let (file, path) = tmp.keep()?;
    serde_json::to_writer(file, &calls)?;

    ctx.say("sending restart command..").await?;

    println!("!restart,path={}", path.to_string_lossy());

    Ok(())
}
