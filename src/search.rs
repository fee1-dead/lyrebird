use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use poise::serenity_prelude::{
    ComponentInteraction, ComponentInteractionCollector, ComponentInteractionData,
    ComponentInteractionDataKind, CreateActionRow, CreateSelectMenu, CreateSelectMenuKind,
    CreateSelectMenuOption, EditMessage, Message,
};
use poise::CreateReply;
use serenity::prelude::Mutex;
use songbird::input::YoutubeDl;
use songbird::Call;
use tokio::process::Command;
use tokio::spawn;
use tokio::time::timeout;
use tracing::{debug, error};

use crate::play::{play_multiple, Output};
use crate::vc::enter_vc;
use crate::{CommandResult, Context, Data, DiscordContext};

crate::commands!(search);

pub struct SearchResult {
    artist: Option<String>,
    title: Option<String>,
    url: String,
}

impl SearchResult {
    pub fn title_or_url(&self) -> &str {
        self.title.as_ref().map(|x| x.as_str()).unwrap_or(&self.url)
    }
}

#[poise::command(slash_command)]
/// Returns a list of songs from a given search term.
pub async fn search(ctx: Context<'_>, #[rest] keyword: String) -> CommandResult {
    ctx.defer().await?;
    let cmd = Command::new("yt-dlp")
        .arg("-j")
        .arg("-s")
        .arg("--flat-playlist")
        .arg(format!("ytsearch10:{keyword}"))
        .output()
        .await?;
    let mut results = Vec::new();
    for bytes in cmd.stdout.split(|x| *x == b'\n') {
        if bytes.is_empty() {
            continue;
        }
        let out = serde_json::from_slice::<Output>(bytes)?;
        debug!(?out);
        if !out.is_playable() {
            continue;
        }
        results.push(SearchResult {
            artist: out.channel,
            title: out.title,
            url: out.url,
        });
    }

    let mut embed = serenity::builder::CreateEmbed::default()
        .title(format!("Search results for \"{keyword}\""));
    for (i, result) in results.iter().enumerate() {
        embed = embed.field(
            format!(
                "{}: {}",
                i + 1,
                result.title.as_ref().unwrap_or(&result.url.clone())
            ),
            result.artist.as_ref().unwrap_or(&String::new()),
            false,
        );
    }
    let options = results
        .iter()
        .enumerate()
        .map(|(i, x)| CreateSelectMenuOption::new(format!("{} - {}", i + 1, x.title_or_url()), i.to_string()))
        .collect();
    let select = CreateSelectMenu::new("sel", CreateSelectMenuKind::String { options }).min_values(1).max_values(100);
    let msg = ctx
        .send(
            CreateReply::default()
                .embed(embed)
                .components(vec![CreateActionRow::SelectMenu(select)]),
        )
        .await?
        .into_message()
        .await?;

    let collector = msg.await_component_interactions(ctx.discord());

    enter_vc(ctx, true, move |handler, ctx| async move {
        handle_search_responses(msg, ctx, handler, collector, results).await
    })
    .await?;

    Ok(())
}

async fn handle_search_responses(
    mut msg: Message,
    ctx: Context<'_>,
    handler: Arc<Mutex<Call>>,
    rxns: ComponentInteractionCollector,
    results: Vec<SearchResult>,
) -> CommandResult {
    if let Ok(Some(ComponentInteraction {
        data:
            ComponentInteractionData {
                kind: ComponentInteractionDataKind::StringSelect { values },
                ..
            },
        ..
    })) = timeout(Duration::from_secs(60), rxns.next()).await
    {
        let values = values
            .into_iter()
            .map(|x| x.parse::<usize>())
            .collect::<Result<Vec<_>, _>>()?;
        /*
            I wanted to deduplicate these while keeping them in order,
            and we figured out how to do it on discord. But actually I don't want to deduplicate it.
            This chunk of code is really nice. So I have kept it here.

            let values = values.into_iter().filter({
                let mut hs = HashSet::new();
                move |x| hs.insert(*x)
            });
        */

        let inputs = values
            .iter()
            .map(|x| YoutubeDl::new(ctx.data().client.clone(), results[*x].url.clone()).into())
            .collect::<Vec<_>>();

        play_multiple(ctx, inputs, &mut *handler.lock().await).await?;
    }
    msg.edit(ctx, EditMessage::new().components(vec![])).await?;
    Ok(())
}
