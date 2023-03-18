use std::env;
use std::num::NonZeroU64;

use serenity::model::prelude::UserId;
use serenity::prelude::GatewayIntents;

use songbird::SerenityInit;
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
    search::register_commands(&mut v);

    v.push(register());
    v
}

#[poise::command(prefix_command)]
pub async fn register(ctx: Context<'_>) -> CommandResult {
    poise::builtins::register_application_commands_buttons(ctx).await?;
    Ok(())
}

async fn main_inner() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    poise::FrameworkBuilder::default()
        .client_settings(|c| c.register_songbird())
        .options(poise::FrameworkOptions {
            commands: all_commands(),
            owners: [UserId(NonZeroU64::new(468253584421552139).unwrap())]
                .into_iter()
                .collect(),
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("~".into()),
                ..Default::default()
            },
            ..Default::default()
        })
        .token(env::var("DISCORD_TOKEN").expect("Expected a token in the environment"))
        .intents(GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT)
        .user_data_setup(|_ctx, _ready, _framework| {
            Box::pin(async move {
                Ok(Data {
                    client: reqwest::Client::new(),
                })
            })
        })
        .run()
        .await
        .unwrap();
}
