use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use poise::futures_util::StreamExt;
use poise::serenity_prelude::{
    Context as DiscordContext, EditMessage, Message, ReactionCollector, ReactionType,
};
use serenity::prelude::Mutex;
use songbird::Call;
use tokio::spawn;
use tokio::time::timeout;

use crate::metadata::{format_duration, format_metadata, AuxMetadataKey};
use crate::vc::enter_vc;
use crate::{CommandResult, Context, Error};

crate::commands!(queue);

pub async fn retrieve_queue(h: &Call, mut range: Range<usize>) -> String {
    range.end = range.end.min(h.queue().len());
    let start = range.start;
    let mut reply = String::new();
    for (n, song) in h
        .queue()
        .current_queue()
        .get(range)
        .unwrap()
        .into_iter()
        .enumerate()
    {
        let n = n + start;
        let map = song.typemap().read().await;
        let metadata = map.get::<AuxMetadataKey>().unwrap();
        let duration = &metadata.duration;
        if !reply.is_empty() {
            reply.push('\n');
        }

        let duration = match duration {
            Some(duration) => format_duration(*duration),
            None => "unknown".into(),
        };

        let (left, right) = if n == 0 {
            let time = match song.get_info().await {
                Ok(info) => format!(" - {} / {duration}", format_duration(info.position),),
                Err(_) => "- Error getting time".into(),
            };
            ("**Now Playing**".into(), time)
        } else {
            (n.to_string(), String::new())
        };

        reply.push_str(&format!("{left}: {}{right}", format_metadata(metadata),));
    }
    reply
}

fn start_pagination(
    mut msg: Message,
    ctx: DiscordContext,
    handler: Arc<Mutex<Call>>,
    rxns: ReactionCollector,
) {
    spawn(async move {
        // Two minutes waiting on reactions
        let _ = timeout(Duration::from_secs(120), async move {
            let page = Arc::new(AtomicUsize::new(0));
            let mut stream = rxns.stream();
            while let Some(x) = stream.next().await {
                if x.emoji.unicode_eq("⬅️") {
                    let new = page.load(Ordering::SeqCst).saturating_sub(1);
                    page.store(new, Ordering::SeqCst);
                } else if x.emoji.unicode_eq("➡️") {
                    let new = page.load(Ordering::SeqCst).saturating_add(1);
                    page.store(new, Ordering::SeqCst);
                }

                let pg = page.load(Ordering::SeqCst);
                let h = handler.lock().await;
                let start = pg * 10;
                let end = start + 10;
                let message = if start >= h.queue().len() {
                    "Index out of bounds.".into()
                } else {
                    retrieve_queue(&h, start..end).await
                };
                drop(h);
                msg.edit(&ctx, EditMessage::new().content(message)).await?;
                msg.delete_reactions(&ctx).await?;
                msg.react(&ctx, ReactionType::Unicode("⬅️".into())).await?;
                msg.react(&ctx, ReactionType::Unicode("➡️".into())).await?;
            }
            Ok::<_, Error>(())
        })
        .await;
    });
}

#[poise::command(slash_command)]
async fn queue(ctx: Context<'_>) -> CommandResult {
    enter_vc(ctx, false, |handler, ctx| async move {
        let hlock = handler.lock().await;

        if hlock.queue().is_empty() {
            drop(hlock);
            ctx.say("queue is empty").await?;
            return Ok(());
        }

        let msg = ctx.say(retrieve_queue(&hlock, 0..10).await).await?;

        drop(hlock);

        let discord = ctx.discord();
        let msg = msg.into_message().await?;
        msg.react(discord, ReactionType::Unicode("⬅️".into()))
            .await?;
        msg.react(discord, ReactionType::Unicode("➡️".into()))
            .await?;

        let rxns = msg.await_reaction(ctx.discord());
        start_pagination(msg, discord.clone(), handler, rxns);

        Ok(())
    })
    .await
}
