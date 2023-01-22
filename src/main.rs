use std::collections::VecDeque;
use std::env;
use std::error::Error;
use std::future::Future;
use std::num::NonZeroU64;
use std::sync::Arc;
use std::time::Duration;

use serenity::all::{CommandDataOption, CommandDataOptionValue as Val, CommandInteraction, Interaction};
use serenity::builder::{
    CreateCommand, CreateCommandOption, CreateInteractionResponse, CreateInteractionResponseMessage, EditInteractionResponse,
};


use serenity::gateway::ActivityData;
use serenity::model::prelude::{GuildId, UserId};
use serenity::model::user::OnlineStatus;
use serenity::prelude::Mutex;
use songbird::tracks::{Queued, PlayMode};
use songbird::typemap::TypeMapKey;
// This trait adds the `register_songbird` and `register_songbird_with` methods
// to the client builder below, making it easy to install this voice client.
// The voice client can be retrieved in any command using `songbird::get(ctx).await`.
use songbird::{Call, SerenityInit, EventContext};

// Import the `Context` to handle commands.
use serenity::client::Context;

use serenity::{
    async_trait,
    client::{Client, EventHandler},
    model::gateway::Ready,
    prelude::GatewayIntents,
};
use songbird::input::{AuxMetadata, Input, YoutubeDl};

use tracing::{info, warn};

pub type CommandResult = std::result::Result<(), Box<dyn Error + Send + Sync>>;

struct Handler {
    client: reqwest::Client,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);
        ctx.set_presence(Some(ActivityData::watching("you")), OnlineStatus::Online);
        GuildId(NonZeroU64::new(1051160112036851733).unwrap())
            .set_application_commands(ctx, register_commands())
            .await
            .unwrap();
    }
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            info!("Received command interaction: {:#?}", command);

            macro_rules! commands {
                (@opt($s:ident, $l: literal)) => ($l);
                (@opt($s:ident)) => (stringify!($s));
                ($($name:ident$([$s: literal])?),*$(,)?) => {
                    match command.data.name.as_str() {
                        $(
                            commands!(@opt($name$(, $s)?)) => $name(&ctx, self, command).await,
                        )*
                        _ => unreachable!(),
                    }
                };
            }

            if let Err(e) = commands! {
                play, splay, join, leave, deafen, undeafen, mv["move"], swap, skip, remove, pause, resume, queue,
            } {
                warn!(%e, "error handling command")
            }
        }
    }
}
fn main() {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(main_inner());
}

fn register_commands() -> Vec<CreateCommand> {
    use serenity::model::prelude::CommandOptionType::*;

    vec![
        CreateCommand::new("play")
            .description("queues a song to play")
            .add_option(CreateCommandOption::new(String, "url", "url of the song").required(true)),
        CreateCommand::new("splay")
            .description("search for song")
            .add_option(CreateCommandOption::new(String, "query", "search query").required(true)),
        CreateCommand::new("join").description("join voice channel"),
        CreateCommand::new("leave").description("leave voice channel"),
        CreateCommand::new("undeafen").description("undeafen"),
        CreateCommand::new("swap")
            .description("swap two tracks")
            .add_option(CreateCommandOption::new(Integer, "a", "swap from").required(true))
            .add_option(CreateCommandOption::new(Integer, "b", "swap to").required(true)),
        CreateCommand::new("move")
            .description("move track")
            .add_option(CreateCommandOption::new(Integer, "from", "move from").required(true))
            .add_option(CreateCommandOption::new(Integer, "to", "move to").required(true)),
        CreateCommand::new("skip").description("skip current track"),
        CreateCommand::new("remove")
            .description("remove track")
            .add_option(
                CreateCommandOption::new(Integer, "index", "index to remove").required(true),
            ),
        CreateCommand::new("pause").description("pause current music"),
        CreateCommand::new("resume").description("resume playing"),
        CreateCommand::new("queue").description("list queue"),
    ]
}

pub async fn simple_reply(c: &CommandInteraction, ctx: &Context, message: &str) {
    if let Err(x) = c
        .create_response(
            ctx,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new().content(message),
            ),
        )
        .await
    {
        warn!(%x, %message, "unable to create command result");
    }
}

async fn main_inner() {
    tracing_subscriber::fmt::init();

    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler { client: reqwest::Client::new() })
        .register_songbird()
        .await
        .expect("Err creating client");

    tokio::spawn(async move {
        let _ = client
            .start()
            .await
            .map_err(|why| println!("Client ended: {:?}", why));
    });

    let _ = tokio::signal::ctrl_c().await;
    println!("Received Ctrl-C, shutting down.");
}

async fn common_voice<
    F: FnOnce(Arc<Mutex<Call>>, CommandInteraction) -> T,
    T: Future<Output = CommandResult>,
>(
    ctx: &Context,
    c: CommandInteraction,
    autojoin: bool,
    f: F,
) -> CommandResult {
    let guild_id = c.guild_id.unwrap();

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    let handler_lock = if autojoin {
        match try_join(
            ctx,
            c.user.id,
            guild_id,
            false,
        )
        .await
        {
            Ok(x) => x,
            Err(e) => {
                simple_reply(&c, ctx, &format!("failed to autojoin: {e:?}")).await;

                return Ok(());
            }
        }
    } else {
        match manager.get(guild_id) {
            Some(handler) => handler,
            None => {
                simple_reply(&c, ctx, "Not in a voice channel").await;

                return Ok(());
            }
        }
    };

    f(handler_lock, c).await
}

async fn deafen(ctx: &Context, _: &Handler, c: CommandInteraction) -> CommandResult {
    common_voice(ctx, c, false, |handler, c| async move {
        let mut handler = handler.lock().await;
        if handler.is_deaf() {
            simple_reply(&c, ctx, "Already deafened").await;
        } else {
            if let Err(e) = handler.deafen(true).await {
                simple_reply(&c, ctx, &format!("Failed to deafen: {e:?}")).await;
            } else {
                simple_reply(&c, ctx, "Deafened").await;
            }
        }

        Ok(())
    })
    .await
}

async fn pause(ctx: &Context, _: &Handler, c: CommandInteraction) -> CommandResult {
    common_voice(ctx, c, false, |handler, c| async move {
        if let Err(e) = handler.lock().await.queue().pause() {
            warn!(?e, "failed to pause");
        }

        simple_reply(&c, ctx, "paused").await;

        Ok(())
    })
    .await
}

async fn resume(ctx: &Context, _: &Handler, c: CommandInteraction) -> CommandResult {
    common_voice(ctx, c, false, |handler, c| async move {
        if let Err(e) = handler.lock().await.queue().resume() {
            warn!(?e, "failed to resume");
        }

        simple_reply(&c, ctx, "resumed").await;

        Ok(())
    })
    .await
}

async fn try_join(
    ctx: &Context,
    user: UserId,
    guild: GuildId,
    must_join: bool,
) -> Result<Arc<Mutex<Call>>, &'static str> {
    let manager = songbird::get(ctx)
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

    let channel_id = guild.to_guild_cached(ctx).unwrap()
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

        handler.lock().await.add_global_event(songbird::Event::Track(songbird::TrackEvent::Error), ErrorHandler);

    // TODO: on first join we need to install some event handlers
    // h.lock().await.add_global_event(Event::Track(TrackEvent::Play), action);

    Ok(handler)
}

pub struct ErrorHandler;

#[async_trait]
impl songbird::EventHandler for ErrorHandler {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<songbird::Event> {
        match ctx {
            EventContext::Track(e) => {
                for t in *e {
                    match &t.0.playing {
                        PlayMode::Errored(e) => {
                            warn!(%e, "track errored");
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        None
    }
}

async fn join(ctx: &Context, _: &Handler, c: CommandInteraction) -> CommandResult {
    match try_join(
        ctx,
        c.user.id,
        c.guild_id.unwrap(),
        true,
    )
    .await {
        Ok(_) => {
            simple_reply(&c, ctx, "Joined").await;
        }
        Err(e) => {
            simple_reply(&c, ctx, &format!("failed to join: {e}")).await;

            return Ok(());
        }
    }
    Ok(())
}

async fn leave(ctx: &Context, _: &Handler, c: CommandInteraction) -> CommandResult {
    let guild_id = c.guild_id.unwrap();

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();
    let has_handler = manager.get(guild_id).is_some();

    if has_handler {
        if let Err(e) = manager.remove(guild_id).await {
            simple_reply(&c, ctx, &format!("Failed: {e:?}")).await;
        } else {
            simple_reply(&c, ctx, "Left voice channel").await;
        }
    } else {
        simple_reply(&c, ctx, "Not in a voice channel").await;
    }

    Ok(())
}

async fn skip(ctx: &Context, _: &Handler, c: CommandInteraction) -> CommandResult {
    common_voice(ctx, c, false, |handler_lock, c| async move {
        let handler = handler_lock.lock().await;
        if handler.queue().is_empty() {
            simple_reply(&c, ctx, "queue is empty").await;
        } else {
            let _ = handler.queue().skip();
            simple_reply(&c, ctx, "skipped").await;
        }
        Ok(())
    })
    .await
}

async fn queue_modify<F: FnOnce(usize, usize, &mut VecDeque<Queued>) -> String>(
    ctx: &Context,
    c: CommandInteraction,
    f: F,
) -> CommandResult {
    let (from, to) = match &*c.data.options {
        [CommandDataOption {
            value: Val::Integer(from),
            ..
        }, CommandDataOption {
            value: Val::Integer(to),
            ..
        }] => (*from, *to),
        _ => {
            simple_reply(&c, ctx, "invalid arguments").await;
            return Ok(());
        }
    };

    if from == 0 || to == 0 {
        simple_reply(&c, ctx, "Cannot move the current song").await;
        return Ok(());
    }

    common_voice(ctx, c, false, |handler_lock, c| async move {
        let handler = handler_lock.lock().await;
        let m = handler.queue().modify_queue(|x| f(from as _, to as _, x));
        simple_reply(&c, ctx, &m).await;
        Ok(())
    })
    .await
}

async fn mv(ctx: &Context, _: &Handler, c: CommandInteraction) -> CommandResult {
    queue_modify(ctx, c, |from, to, x| {
        if let Some(song) = x.remove(from) {
            if to > x.len() {
                x.push_back(song);
            } else {
                x.insert(to, song);
            }
            "Sucess".into()
        } else {
            format!("Failed: index out of bounds for {from}")
        }
    })
    .await
}

async fn swap(ctx: &Context, _: &Handler, c: CommandInteraction) -> CommandResult {
    queue_modify(ctx, c, |from, to, x| {
        if from >= x.len() || to >= x.len() {
            format!("Failed: index out of bounds for {from} or {to}")
        } else {
            x.swap(from, to);
            "Sucess".into()
        }
    })
    .await
}

pub struct AuxMetadataKey;

impl TypeMapKey for AuxMetadataKey {
    type Value = AuxMetadata;
}

async fn play_common(
    ctx: &Context,
    h: &Handler,
    c: CommandInteraction,
    mk: fn(&Handler, String) -> Input,
    url: bool,
) -> CommandResult {
    let term = match &*c.data.options {
        [CommandDataOption {
            value: Val::String(url),
            ..
        }] => url.clone(),
        _ => {
            simple_reply(
                &c,
                ctx,
                if url {
                    "Must provide a URL to a video or audio"
                } else {
                    "invalid arguments"
                },
            )
            .await;

            return Ok(());
        }
    };
    if url && !term.starts_with("http") {
        simple_reply(&c, ctx, "Must provide a valid url").await;

        return Ok(());
    }
    c.create_response(ctx, CreateInteractionResponse::Defer(CreateInteractionResponseMessage::new().content("resolving"))).await?;
    common_voice(ctx, c, true, |handler_lock, c| async move {
        let mut handler = handler_lock.lock().await;

        let input = mk(h, term);
        let mut source: Input = input.into();
        let metadata = source.aux_metadata().await?;
        let msg = format!(
            "Queued: {}",
            format_metadata(&metadata),
        );
        let handle = handler.enqueue_input(source).await;
        handle.typemap().write().await.insert::<AuxMetadataKey>(metadata);
        c.edit_response(ctx, EditInteractionResponse::new().content(msg)).await?;
        Ok(())
    })
    .await
}

async fn splay(ctx: &Context, h: &Handler, c: CommandInteraction) -> CommandResult {
    play_common(
        ctx,
        h,
        c,
        |h, term| YoutubeDl::new(h.client.clone(), format!("ytsearch1:{term}")).into(),
        false,
    )
    .await
}

async fn play(ctx: &Context, h: &Handler, c: CommandInteraction) -> CommandResult {
    play_common(ctx, h, c, |h, url| YoutubeDl::new(h.client.clone(), url).into(), true).await
}

fn format_metadata(AuxMetadata { title, artist, .. }: &AuxMetadata) -> String {
    format!(
        "{} - {}",
        artist.as_deref().unwrap_or("unknown artist"),
        title.as_deref().unwrap_or("unknown title")
    )
}

fn format_duration(x: Duration) -> String {
    let secs = x.as_secs();
    let mins = secs / 60;
    let hours = mins / 60;
    let mins = mins % 60;
    let secs = secs % 60;
    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, mins, secs)
    } else if mins > 0 {
        format!("{}:{:02}", mins, secs)
    } else {
        format!("{}s", secs)
    }
}

async fn queue(ctx: &Context, _: &Handler, c: CommandInteraction) -> CommandResult {
    common_voice(ctx, c, false, |handler, c| async move {
        let handler = handler.lock().await;

        if handler.queue().is_empty() {
            simple_reply(&c, ctx, "queue is empty").await;

            return Ok(());
        }

        let mut reply = String::new();
        for (n, song) in handler.queue().current_queue().into_iter().enumerate() {
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

        simple_reply(&c, ctx, &reply).await;

        Ok(())
    })
    .await
}

async fn remove(ctx: &Context, _: &Handler, c: CommandInteraction) -> CommandResult {
    let index = match &*c.data.options {
        [CommandDataOption {
            value: Val::Integer(i),
            ..
        }] => (*i) as usize,
        _ => {
            simple_reply(&c, ctx, "must provide valid index").await;

            return Ok(());
        }
    };

    common_voice(ctx, c, false, |handler, c| async move {
        let handler = handler.lock().await;

        let result = handler.queue().modify_queue(|x| {
            if let Some(track) = x.remove(index) {
                if let Err(e) = track.stop() {
                    Err(format!("Failed to stop track: {:?}", e))
                } else {
                    Ok(track)
                }
            } else {
                Err(format!("No track at index {index}"))
            }
        });

        match result {
            Ok(track) => {
                let map = track.typemap().read().await;
                let metadata = map.get::<AuxMetadataKey>().unwrap();
                simple_reply(&c, ctx, &format!("Removed: {}", format_metadata(metadata))).await;
            }
            Err(e) => simple_reply(&c, ctx, &e).await,
        }

        Ok(())
    })
    .await
}

async fn undeafen(ctx: &Context, _: &Handler, c: CommandInteraction) -> CommandResult {
    common_voice(ctx, c, false, |handler_lock, c| async move {
        let mut handler = handler_lock.lock().await;
        if let Err(e) = handler.deafen(false).await {
            simple_reply(&c, ctx, &format!("Failed: {e:?}")).await;
        } else {
            simple_reply(&c, ctx, "undeafened").await;
        }

        Ok(())
    })
    .await
}
