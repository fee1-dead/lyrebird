use std::collections::VecDeque;
use std::env;
use std::error::Error;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use serenity::builder::CreateApplicationCommands;
use serenity::futures::future::BoxFuture;
use serenity::model::prelude::command::Command;
use serenity::model::prelude::interaction::application_command::{
    ApplicationCommandInteraction, CommandDataOption, CommandDataOptionValue as Val,
};
use serenity::model::prelude::interaction::Interaction;
use serenity::model::prelude::{Activity, Guild, UserId, GuildId};
use serenity::model::user::OnlineStatus;
use serenity::prelude::Mutex;
use songbird::tracks::Queued;
// This trait adds the `register_songbird` and `register_songbird_with` methods
// to the client builder below, making it easy to install this voice client.
// The voice client can be retrieved in any command using `songbird::get(ctx).await`.
use songbird::{Call, SerenityInit};

// Import the `Context` to handle commands.
use serenity::client::Context;

use serenity::{
    async_trait,
    client::{Client, EventHandler},
    model::gateway::Ready,
    prelude::GatewayIntents,
};
use songbird::input::{Input, Metadata, Restartable};

use tracing::{info, warn};

pub type CommandResult = std::result::Result<(), Box<dyn Error + Send + Sync>>;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);
        ctx.set_presence(Some(Activity::watching("you")), OnlineStatus::Online)
            .await;
        GuildId(1051160112036851733).set_application_commands(ctx, register_commands).await.unwrap();
    }
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            info!("Received command interaction: {:#?}", command);

            macro_rules! commands {
                ($($name:ident),*$(,)?) => {
                    match command.data.name.as_str() {
                        $(
                            stringify!($name) => $name(&ctx, command).await,
                        )*
                        _ => unreachable!(),
                    }
                };
            }

            if let Err(e) = commands! {
                play, splay, join, leave, deafen, undeafen, mv, swap, skip, remove, pause, resume, queue,
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

fn register_commands(c: &mut CreateApplicationCommands) -> &mut CreateApplicationCommands {
    use serenity::model::prelude::command::CommandOptionType::*;

    c.create_application_command(|c| {
        c.name("play")
            .description("queues a song to play")
            .create_option(|x| {
                x.name("url")
                    .description("URL of the song")
                    .required(true)
                    .kind(String)
            })
    })
    .create_application_command(|c| {
        c.name("splay")
            .description("search for song to play")
            .create_option(|x| {
                x.name("query")
                    .description("search query")
                    .required(true)
                    .kind(String)
            })
    })
    .create_application_command(|c| c.name("join").description("join voice channel"))
    .create_application_command(|c| c.name("leave").description("leave voice channel"))
    .create_application_command(|c| c.name("undeafen").description("undeafen"))
    .create_application_command(|c| c.name("deafen").description("deafen"))
    .create_application_command(|c| {
        c.name("swap")
            .description("swap two tracks")
            .create_option(|x| {
                x.name("a")
                    .description("index to swap")
                    .required(true)
                    .kind(Integer)
            })
            .create_option(|x| {
                x.name("b")
                    .description("index to swap")
                    .required(true)
                    .kind(Integer)
            })
    })
    .create_application_command(|c| {
        c.name("mv")
            .description("move music")
            .create_option(|x| {
                x.name("from")
                    .description("index to move from")
                    .required(true)
                    .kind(Integer)
            })
            .create_option(|x| {
                x.name("to")
                    .description("index to move to")
                    .required(true)
                    .kind(Integer)
            })
    })
    .create_application_command(|c| c.name("skip").description("skip music"))
    .create_application_command(|c| {
        c.name("remove")
            .description("remove track")
            .create_option(|x| {
                x.name("index")
                    .description("index of music to remove")
                    .required(true)
                    .kind(Integer)
            })
    })
    .create_application_command(|c| c.name("pause").description("pause currently playing music"))
    .create_application_command(|c| c.name("resume").description("resume playing current music"))
    .create_application_command(|c| c.name("queue").description("list queue contents"))
}

pub async fn simple_reply(c: &ApplicationCommandInteraction, ctx: &Context, message: &str) {
    if let Err(x) = c
        .create_interaction_response(ctx, |x| {
            x.interaction_response_data(|data| data.content(message))
        })
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
        .event_handler(Handler)
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
    F: FnOnce(Arc<Mutex<Call>>, ApplicationCommandInteraction) -> T,
    T: Future<Output = CommandResult>,
>(
    ctx: &Context,
    c: ApplicationCommandInteraction,
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
            guild_id.to_guild_cached(ctx).unwrap(),
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

async fn deafen(ctx: &Context, c: ApplicationCommandInteraction) -> CommandResult {
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

async fn pause(ctx: &Context, c: ApplicationCommandInteraction) -> CommandResult {
    common_voice(ctx, c, false, |handler, c| async move {
        if let Err(e) = handler.lock().await.queue().pause() {
            warn!(?e, "failed to pause");
        }

        simple_reply(&c, ctx, "paused").await;

        Ok(())
    })
    .await
}

async fn resume(ctx: &Context, c: ApplicationCommandInteraction) -> CommandResult {
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
    guild: Guild,
    must_join: bool,
) -> Result<Arc<Mutex<Call>>, &'static str> {
    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    if let Some(call) = manager.get(guild.id) {
        if must_join {
            return Err("already in a voice channel");
        } else {
            return Ok(call);
        }
    }

    let channel_id = guild
        .voice_states
        .get(&user)
        .and_then(|voice_state| voice_state.channel_id);

    let connect_to = match channel_id {
        Some(channel) => channel,
        None => {
            return Err("you are not in a voice channel");
        }
    };

    let (call, _) = manager.join(guild.id, connect_to).await;

    Ok(call)
}

async fn join(ctx: &Context, c: ApplicationCommandInteraction) -> CommandResult {
    if let Err(why) = try_join(
        ctx,
        c.user.id,
        c.guild_id.unwrap().to_guild_cached(ctx).unwrap(),
        true,
    )
    .await
    {
        simple_reply(&c, ctx, &format!("Failed to join: {why}")).await;
    } else {
        simple_reply(&c, ctx, "joined").await;
    }

    Ok(())
}

async fn leave(ctx: &Context, c: ApplicationCommandInteraction) -> CommandResult {
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

async fn skip(ctx: &Context, c: ApplicationCommandInteraction) -> CommandResult {
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
    c: ApplicationCommandInteraction,
    f: F,
) -> CommandResult {
    let (from, to) = match &*c.data.options {
        [CommandDataOption {
            resolved: Some(Val::Integer(from)),
            ..
        }, CommandDataOption {
            resolved: Some(Val::Integer(to)),
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

async fn mv(ctx: &Context, c: ApplicationCommandInteraction) -> CommandResult {
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

async fn swap(ctx: &Context, c: ApplicationCommandInteraction) -> CommandResult {
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

async fn play_common(
    ctx: &Context,
    c: ApplicationCommandInteraction,
    mk: fn(String) -> BoxFuture<'static, songbird::input::error::Result<Restartable>>,
    url: bool,
) -> CommandResult {
    let term = match &*c.data.options {
        [CommandDataOption {
            resolved: Some(Val::String(url)),
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
    common_voice(ctx, c, true, |handler_lock, c| async move {
        let mut handler = handler_lock.lock().await;

        let input = match mk(term).await {
            Ok(input) => input,
            Err(why) => {
                println!("Err starting source: {:?}", why);

                simple_reply(&c, ctx, "error sourcing ffmpeg").await;

                return Ok(());
            }
        };
        let source: Input = input.into();
        let track = format_metadata(&source.metadata);
        handler.enqueue_source(source);
        simple_reply(&c, ctx, &format!("Queued {track}.")).await;
        Ok(())
    })
    .await
}

async fn splay(ctx: &Context, c: ApplicationCommandInteraction) -> CommandResult {
    play_common(
        ctx,
        c,
        |term| Box::pin(Restartable::ytdl_search(term, true)),
        false,
    )
    .await
}

async fn play(ctx: &Context, c: ApplicationCommandInteraction) -> CommandResult {
    play_common(ctx, c, |url| Box::pin(Restartable::ytdl(url, true)), true).await
}

fn format_metadata(Metadata { title, artist, .. }: &Metadata) -> String {
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

async fn queue(ctx: &Context, c: ApplicationCommandInteraction) -> CommandResult {
    common_voice(ctx, c, false, |handler, c| async move {
        let handler = handler.lock().await;

        if handler.queue().is_empty() {
            simple_reply(&c, ctx, "queue is empty").await;

            return Ok(());
        }

        let mut reply = String::new();
        for (n, song) in handler.queue().current_queue().into_iter().enumerate() {
            let metadata = song.metadata();
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

async fn remove(ctx: &Context, c: ApplicationCommandInteraction) -> CommandResult {
    let index = match &*c.data.options {
        [CommandDataOption {
            resolved: Some(Val::Integer(i)),
            ..
        }] => (*i) as usize,
        _ => {
            simple_reply(&c, ctx, "must provide valid index").await;

            return Ok(());
        }
    };

    common_voice(ctx, c, false, |handler, c| async move {
        let handler = handler.lock().await;

        let message = handler.queue().modify_queue(|x| {
            if let Some(track) = x.remove(index) {
                if let Err(e) = track.stop() {
                    format!("Failed to stop track: {:?}", e)
                } else {
                    format!(
                        "Removed track: {}",
                        track.metadata().title.as_deref().unwrap_or("unknown title")
                    )
                }
            } else {
                format!("No track at index {}", index)
            }
        });

        simple_reply(&c, ctx, &message).await;

        Ok(())
    })
    .await
}

async fn undeafen(ctx: &Context, c: ApplicationCommandInteraction) -> CommandResult {
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
