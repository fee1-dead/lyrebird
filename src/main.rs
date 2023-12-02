use std::env;
use std::num::NonZeroU64;
use std::sync::Arc;

use reqwest::Client;
use restart::CallData;
use serenity::client::ClientBuilder;
use serenity::gateway::ActivityData;
use serenity::model::prelude::UserId;
use serenity::prelude::GatewayIntents;

use songbird::id::{ChannelId, GuildId};
use songbird::{SerenityInit, Songbird};
use tokio::fs;
use tracing::warn;
use tracing_subscriber::EnvFilter;

type Error = color_eyre::Report;

pub type CommandResult = std::result::Result<(), Error>;

type Context<'a> = poise::Context<'a, Data, Error>;

pub type Command = poise::Command<Data, Error>;

pub(crate) use serenity::client::Context as DiscordContext;

mod list;
mod metadata;
mod play;
mod queue;
mod restart;
mod search;
mod track;
mod vc;
mod ws;

macro_rules! commands {
    ($($i: ident),*$(,)?) => {
        pub fn register_commands(v: &mut Vec<crate::Command>) {
            v.extend([ $( $i() ),* ]);
        }
    }
}

pub(crate) use commands;

pub struct Data {
    client: reqwest::Client,
}

fn main() {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(main_inner());
}

fn all_commands() -> Vec<Command> {
    let mut v = Vec::new();

    play::register_commands(&mut v);
    list::register_commands(&mut v);
    track::register_commands(&mut v);
    vc::register_commands(&mut v);
    queue::register_commands(&mut v);
    restart::register_commands(&mut v);
    search::register_commands(&mut v);

    v.push(register());
    v.push(help());

    v
}

#[poise::command(prefix_command)]
pub async fn register(ctx: Context<'_>) -> CommandResult {
    poise::builtins::register_application_commands_buttons(ctx).await?;
    Ok(())
}

#[poise::command(prefix_command, track_edits, slash_command)]
pub async fn help(
    ctx: Context<'_>,
    #[description = "Specific command to show help about"] command: Option<String>,
) -> Result<(), Error> {
    let config = poise::builtins::HelpConfiguration {
        extra_text_at_bottom: "\
Type /help command for more info on a specific command.\n\n\
Source code available at: https://github.com/fee1-dead/lyrebird",
        ..Default::default()
    };
    poise::builtins::help(ctx, command.as_deref(), config).await?;
    Ok(())
}

async fn maybe_recover(ctx: &DiscordContext, client: Client) {
    if let Ok(x) = env::var("RESTART_RECOVER_PATH") {
        let songbird = songbird::get(ctx).await.unwrap();
        tokio::spawn(async move {
            if let Err(e) = maybe_recover_inner(songbird, x, client).await {
                warn!("Error occured while recovering: {e}");
            }
        });
    }
}

async fn maybe_recover_inner(
    songbird: Arc<Songbird>,
    path: String,
    client: Client,
) -> color_eyre::Result<()> {
    let f = fs::read_to_string(&path).await?;
    let _ = fs::remove_file(path).await;
    let values: Vec<CallData> = serde_json::from_str(&f)?;
    for CallData {
        guild,
        channel,
        queue,
    } in values
    {
        let Ok(call) = songbird.join(GuildId(guild), ChannelId(channel)).await else {
            continue;
        };
        let mut handler = call.lock().await;
        for q in queue {
            let _ = play::enqueue(client.clone(), q, &mut handler).await;
        }
    }
    Ok(())
}

async fn main_inner() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let bot_owner = env::var("BOT_OWNER_ID").expect("Please set BOT_OWNER_ID");
    let bot_owner = UserId::from(
        NonZeroU64::new(bot_owner.parse().expect("bot owner id not correctly set"))
            .expect("bot owner ID should be non-zero"),
    );

    let framework = poise::FrameworkBuilder::default()
        .setup(|ctx, _ready, _framework| {
            Box::pin(async move {
                let client = reqwest::Client::new();
                maybe_recover(ctx, client.clone()).await;
                Ok(Data { client })
            })
        })
        .options(poise::FrameworkOptions {
            commands: all_commands(),
            owners: [bot_owner].into_iter().collect(),
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("~".into()),
                ..Default::default()
            },
            ..Default::default()
        })
        .build();

    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;
    let client = ClientBuilder::new(token, intents)
        .register_songbird()
        .activity(ActivityData::watching("you"))
        .framework(framework)
        .await;
    client.unwrap().start().await.unwrap();
}
