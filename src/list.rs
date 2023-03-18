use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use poise::futures_util::StreamExt;
use poise::serenity_prelude::{
    ComponentInteractionCollector, CreateActionRow, CreateButton, CreateInteractionResponse,
    EditMessage, Message, ReactionType,
};
use poise::CreateReply;
use serenity::prelude::Mutex;
use songbird::Call;
use tokio::spawn;
use tokio::time::timeout;
use tracing::error;

use crate::metadata::{format_duration, format_metadata, AuxMetadataKey};
use crate::vc::enter_vc;
use crate::{CommandResult, Context, DiscordContext, Error};

crate::commands!(queue);

fn calc_pages(n: usize, page_len: usize) -> usize {
    n.saturating_sub(1) / page_len + 1
}

pub async fn retrieve_queue(h: &Call, page: usize) -> String {
    let mut range = page * 10..(page + 1) * 10;
    range.end = range.end.min(h.queue().len());
    if range.start >= h.queue().len() {
        return "Index out of bounds.".into();
    }

    let start = range.start;
    let mut reply = String::new();
    for (n, song) in h
        .queue()
        .current_queue()
        .get(range)
        .unwrap()
        .iter()
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
    let current_page = page + 1;
    let pages_total = calc_pages(h.queue().len(), 10);
    reply.push_str(&format!(
        "\n\nDisplaying page {current_page} of {pages_total} (10 per page)"
    ));
    reply
}

fn make_buttons(page: usize, len: usize) -> CreateActionRow {
    CreateActionRow::Buttons(vec![
        CreateButton::new("prev_page")
            .emoji(ReactionType::Unicode("⬅️".into()))
            .disabled(page == 0),
        CreateButton::new("next_page")
            .emoji(ReactionType::Unicode("➡️".into()))
            .disabled(page + 1 >= calc_pages(len, 10)),
        CreateButton::new("refresh").emoji('🔄'),
    ])
}

async fn paginate(
    mut msg: Message,
    ctx: &DiscordContext,
    handler: Arc<Mutex<Call>>,
    rxns: ComponentInteractionCollector,
) -> Result<(), Error> {
    let page = Arc::new(AtomicUsize::new(0));
    let mut stream = rxns.stream();
    while let Ok(Some(x)) = timeout(Duration::from_secs(120), stream.next()).await {
        match &*x.data.custom_id {
            "prev_page" => {
                let new = page.load(Ordering::SeqCst).saturating_sub(1);
                page.store(new, Ordering::SeqCst);
            }
            "next_page" => {
                let new = page.load(Ordering::SeqCst).saturating_add(1);
                page.store(new, Ordering::SeqCst);
            }
            "refresh" => {}
            id => tracing::error!("invalid custom id: {id}"),
        }

        let pg = page.load(Ordering::SeqCst);
        let h = handler.lock().await;
        let len = h.queue().len();
        let message = retrieve_queue(&h, pg).await;
        drop(h);
        let newmsg = EditMessage::new()
            .content(message)
            .components(vec![make_buttons(pg, len)]);
        msg.edit(&ctx, newmsg).await?;
        x.create_response(&ctx, CreateInteractionResponse::Acknowledge)
            .await?;
    }

    msg.edit(&ctx, EditMessage::new().components(vec![]))
        .await?;

    Ok::<_, Error>(())
}

fn start_pagination(
    msg: Message,
    ctx: DiscordContext,
    handler: Arc<Mutex<Call>>,
    rxns: ComponentInteractionCollector,
) {
    spawn(async move {
        if let Err(e) = paginate(msg, &ctx, handler, rxns).await {
            error!("error occured in pagination: {e}");
        }
    });
}

#[poise::command(slash_command, prefix_command)]
async fn queue(
    ctx: Context<'_>,
    #[description = "page number to display"] page: Option<usize>,
) -> CommandResult {
    let page = page.map_or(0, |p| p - 1);
    enter_vc(ctx, false, |handler, ctx| async move {
        let hlock = handler.lock().await;
        let len = hlock.queue().len();

        if hlock.queue().is_empty() {
            drop(hlock);
            ctx.say("queue is empty").await?;
            return Ok(());
        }
        let text = retrieve_queue(&hlock, page).await;
        let msg = ctx
            .send(
                CreateReply::new()
                    .content(text)
                    .components(vec![make_buttons(page, len)]),
            )
            .await?;

        drop(hlock);

        let discord = ctx.discord();
        let msg = msg.into_message().await?;

        let rxns = msg.await_component_interaction(ctx.discord());
        start_pagination(msg, discord.clone(), handler, rxns);

        Ok(())
    })
    .await
}
