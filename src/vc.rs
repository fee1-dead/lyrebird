use std::future::Future;
use std::sync::Arc;

use serenity::async_trait;
use serenity::prelude::Mutex;
use songbird::tracks::PlayMode;
use songbird::{Call, EventContext};
use tracing::warn;

use crate::{CommandResult, Context};

crate::commands!(deafen, undeafen, join, leave);

pub struct ErrorHandler;

#[async_trait]
impl songbird::EventHandler for ErrorHandler {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<songbird::Event> {
        if let EventContext::Track(e) = ctx {
            for t in *e {
                if let PlayMode::Errored(e) = &t.0.playing {
                    warn!(%e, "track errored");
                }
            }
        }
        None
    }
}

pub async fn try_join(ctx: Context<'_>, must_join: bool) -> Result<Arc<Mutex<Call>>, &'static str> {
    let guild = ctx.guild_id().unwrap();
    let user = ctx.author().id;
    let manager = songbird::get(ctx.discord())
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    if let Some(call) = manager.get(guild) {
        if must_join {
            return Err("already in a voice channel");
        } else {
            return Ok(call);
        }
    }

    let channel_id = guild
        .to_guild_cached(ctx.discord())
        .unwrap()
        .voice_states
        .get(&user)
        .and_then(|voice_state| voice_state.channel_id);

    let connect_to = match channel_id {
        Some(channel) => channel,
        None => {
            return Err("you are not in a voice channel");
        }
    };

    let handler = manager
        .join(guild, connect_to)
        .await
        .map_err(|_x| "songbird error")?;

    handler.lock().await.add_global_event(
        songbird::Event::Track(songbird::TrackEvent::Error),
        ErrorHandler,
    );

    // TODO: event handlers for play announcement
    // h.lock().await.add_global_event(Event::Track(TrackEvent::Play), action);

    Ok(handler)
}

pub async fn enter_vc<
    'a,
    F: FnOnce(Arc<Mutex<Call>>, Context<'a>) -> T + 'a,
    T: Future<Output = CommandResult> + 'a,
>(
    ctx: Context<'a>,
    autojoin: bool,
    f: F,
) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();

    let manager = songbird::get(ctx.discord())
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    let handler_lock = if autojoin {
        match try_join(ctx, false).await {
            Ok(x) => x,
            Err(e) => {
                ctx.say(format!("failed to autojoin: {e:?}")).await?;

                return Ok(());
            }
        }
    } else {
        match manager.get(guild_id) {
            Some(handler) => handler,
            None => {
                ctx.say("Not in a voice channel").await?;
                return Ok(());
            }
        }
    };

    f(handler_lock, ctx).await
}

#[poise::command(slash_command)]
async fn deafen(ctx: Context<'_>) -> CommandResult {
    enter_vc(ctx, false, |handler, c| async move {
        let mut handler = handler.lock().await;
        if handler.is_deaf() {
            c.say("Already deafened").await?;
        } else if let Err(e) = handler.deafen(true).await {
            c.say(format!("Failed to deafen: {e:?}")).await?;
        } else {
            c.say("Deafened").await?;
        }

        Ok(())
    })
    .await
}

#[poise::command(slash_command)]
async fn undeafen(ctx: Context<'_>) -> CommandResult {
    enter_vc(ctx, false, |handler_lock, ctx| async move {
        let mut handler = handler_lock.lock().await;
        if let Err(e) = handler.deafen(false).await {
            ctx.say(&format!("Failed: {e:?}")).await?;
        } else {
            ctx.say("undeafened").await?;
        }

        Ok(())
    })
    .await
}

#[poise::command(slash_command)]
async fn join(ctx: Context<'_>) -> CommandResult {
    match try_join(ctx, true).await {
        Ok(_) => {
            ctx.say("Joined").await?;
        }
        Err(e) => {
            ctx.say(format!("failed to join: {e}")).await?;
        }
    }
    Ok(())
}

#[poise::command(slash_command)]
async fn leave(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();

    let manager = songbird::get(ctx.discord())
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();
    let has_handler = manager.get(guild_id).is_some();

    if has_handler {
        // TODO replace with let chains
        if let Err(e) = manager.remove(guild_id).await {
            ctx.say(format!("Failed: {e:?}")).await?;
        } else {
            ctx.say("Left voice channel").await?;
        }
    } else {
        ctx.say("Not in a voice channel").await?;
    }

    Ok(())
}
