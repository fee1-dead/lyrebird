use std::collections::VecDeque;
use std::env;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

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
    framework::{
        standard::{
            macros::{command, group},
            Args, CommandResult,
        },
        StandardFramework,
    },
    model::{channel::Message, gateway::Ready},
    prelude::GatewayIntents,
    Result as SerenityResult,
};
use songbird::input::{Metadata, Restartable};

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

#[group]
#[commands(
    deafen, join, leave, splay, play, ping, undeafen, unmute, skip, queue, remove
)]
struct General;

fn main() {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(main_inner());
}

pub struct CommandCommon<'a> {
    context: &'a Context,
    message: &'a Message,
    pub args: Args,
}

impl CommandCommon<'_> {
    pub async fn reply(&self, content: &str) -> SerenityResult<Message> {
        self.message.reply(self.context, content).await
    }
}

async fn main_inner() {
    tracing_subscriber::fmt::init();

    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let framework = StandardFramework::new()
        .configure(|c| c.prefix("~"))
        .group(&GENERAL_GROUP);

    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .framework(framework)
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

async fn common_voice<F: FnOnce(Arc<Mutex<Call>>) -> T, T: Future<Output = CommandResult>>(
    ctx: &Context,
    msg: &Message,
    autojoin: bool,
    f: F,
) -> CommandResult {
    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    let handler_lock = if autojoin {
        match try_join(ctx, msg, false).await {
            Ok(x) => x,
            Err(e) => {
                check_msg(msg.reply(ctx, format!("Failed: {:?}", e)).await);
                return Ok(());
            }
        }
    } else {
        match manager.get(guild_id) {
            Some(handler) => handler,
            None => {
                check_msg(msg.reply(ctx, "Not in a voice channel").await);

                return Ok(());
            }
        }
    };

    f(handler_lock).await
}

#[command]
#[only_in(guilds)]
async fn deafen(ctx: &Context, msg: &Message) -> CommandResult {
    common_voice(ctx, msg, false, |handler| async move {
        let mut handler = handler.lock().await;
        if handler.is_deaf() {
            check_msg(msg.channel_id.say(&ctx.http, "Already deafened").await);
        } else {
            if let Err(e) = handler.deafen(true).await {
                check_msg(
                    msg.channel_id
                        .say(&ctx.http, format!("Failed: {:?}", e))
                        .await,
                );
            }

            check_msg(msg.channel_id.say(&ctx.http, "Deafened").await);
        }

        Ok(())
    })
    .await
}

async fn try_join(
    ctx: &Context,
    msg: &Message,
    must_join: bool,
) -> Result<Arc<Mutex<Call>>, &'static str> {
    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    if let Some(call) = manager.get(guild_id) {
        if must_join {
            return Err("already in a voice channel");
        } else {
            return Ok(call);
        }
    }

    let channel_id = guild
        .voice_states
        .get(&msg.author.id)
        .and_then(|voice_state| voice_state.channel_id);

    let connect_to = match channel_id {
        Some(channel) => channel,
        None => {
            return Err("you are not in a voice channel");
        }
    };

    let (call, _) = manager.join(guild_id, connect_to).await;

    Ok(call)
}

#[command]
#[only_in(guilds)]
async fn join(ctx: &Context, msg: &Message) -> CommandResult {
    if let Err(why) = try_join(ctx, msg, true).await {
        check_msg(msg.reply(ctx, format!("Failed to join: {why}")).await);
    } else {
        if let Err(e) = msg.react(ctx, '👌').await {
            println!("Failed to react: {:?}", e);
        }
    }

    Ok(())
}

#[command]
#[only_in(guilds)]
async fn leave(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();
    let has_handler = manager.get(guild_id).is_some();

    if has_handler {
        if let Err(e) = manager.remove(guild_id).await {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, format!("Failed: {:?}", e))
                    .await,
            );
        }

        check_msg(msg.channel_id.say(&ctx.http, "Left voice channel").await);
    } else {
        check_msg(msg.reply(ctx, "Not in a voice channel").await);
    }

    Ok(())
}

#[command]
async fn ping(context: &Context, msg: &Message) -> CommandResult {
    check_msg(msg.channel_id.say(&context.http, "Pong!").await);

    Ok(())
}

#[command]
#[only_in(guilds)]
async fn skip(ctx: &Context, msg: &Message) -> CommandResult {
    common_voice(ctx, msg, false, |handler_lock| async move {
        let handler = handler_lock.lock().await;
        if handler.queue().is_empty() {
            check_msg(msg.channel_id.say(ctx, "already skipped").await);
        } else {
            let _ = handler.queue().skip();
            check_msg(msg.channel_id.say(&ctx.http, "skipped song").await);
        }
        Ok(())
    })
    .await
}

async fn queue_modify<F: FnOnce(usize, usize, &mut VecDeque<Queued>) -> String>(
    ctx: &Context,
    msg: &Message,
    mut args: Args,
    f: F,
) -> CommandResult {
    let from = match args.single::<usize>() {
        Ok(from) => from,
        Err(_) => {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, "Must provide an index to move from")
                    .await,
            );

            return Ok(());
        }
    };

    let to = match args.single::<usize>() {
        Ok(to) => to,
        Err(_) => {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, "Must provide an index to move to")
                    .await,
            );

            return Ok(());
        }
    };

    if from == 0 || to == 0 {
        check_msg(
            msg.channel_id
                .say(&ctx.http, "Cannot move the currently playing song")
                .await,
        );
        return Ok(());
    }

    common_voice(ctx, msg, false, |handler_lock| async move {
        let handler = handler_lock.lock().await;
        let m = handler.queue().modify_queue(|x| f(from, to, x));
        check_msg(msg.channel_id.say(&ctx.http, m).await);
        Ok(())
    })
    .await
}

#[command]
#[min_args(2)]
#[only_in(guilds)]
async fn r#move(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    queue_modify(ctx, msg, args, |from, to, x| {
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

#[command]
#[min_args(2)]
#[only_in(guilds)]
async fn swap(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    queue_modify(ctx, msg, args, |from, to, x| {
        if from >= x.len() || to >= x.len() {
            format!("Failed: index out of bounds for {from} or {to}")
        } else {
            x.swap(from, to);
            "Sucess".into()
        }
    })
    .await
}

#[command]
#[only_in(guilds)]
#[min_args(1)]
async fn splay(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let term = args.rest();
    common_voice(ctx, msg, true, |handler_lock| async move {
        let mut handler = handler_lock.lock().await;

        let input = match Restartable::ytdl_search(term, true).await {
            Ok(input) => input,
            Err(why) => {
                println!("Err starting source: {:?}", why);

                check_msg(msg.channel_id.say(&ctx.http, "Error sourcing ffmpeg").await);

                return Ok(());
            }
        };
        handler.enqueue_source(input.into());
        check_msg(msg.channel_id.say(&ctx.http, "Added to queue").await);
        Ok(())
    })
    .await
}

#[command]
#[only_in(guilds)]
async fn play(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let url = match args.single::<String>() {
        Ok(url) => url,
        Err(_) => {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, "Must provide a URL to a video or audio")
                    .await,
            );

            return Ok(());
        }
    };

    if !url.starts_with("http") {
        check_msg(
            msg.channel_id
                .say(&ctx.http, "Must provide a valid URL")
                .await,
        );

        return Ok(());
    }

    common_voice(ctx, msg, true, |handler_lock| async move {
        let mut handler = handler_lock.lock().await;

        let input = match Restartable::ytdl(url, true).await {
            Ok(input) => input,
            Err(why) => {
                println!("Err starting source: {:?}", why);

                check_msg(msg.channel_id.say(&ctx.http, "Error sourcing ffmpeg").await);

                return Ok(());
            }
        };
        handler.enqueue_source(input.into());
        check_msg(msg.channel_id.say(&ctx.http, "Added to queue").await);
        Ok(())
    })
    .await
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

#[command]
#[only_in(guilds)]
async fn queue(ctx: &Context, msg: &Message) -> CommandResult {
    common_voice(ctx, msg, false, |handler| async move {
        let handler = handler.lock().await;

        if handler.queue().is_empty() {
            check_msg(msg.channel_id.say(&ctx.http, "Queue is empty").await);

            return Ok(());
        }

        let mut reply = String::new();
        for (n, song) in handler.queue().current_queue().into_iter().enumerate() {
            let Metadata { title, artist, duration, .. } = song.metadata();
            if !reply.is_empty() {
                reply.push('\n');
            }

            let duration = match duration {
                Some(duration) => format_duration(*duration),
                None => "unknown".into(),
            };

            let (left, right) = if n == 0 {
                let time = match song.get_info().await {
                    Ok(info) => format!(
                        " - {} / {duration}",
                        format_duration(info.position),
                    ),
                    Err(_) => "- Error getting time".into(),
                };
                ("**Now Playing**".into(), time)
            } else {
                (n.to_string(), String::new())
            };

            reply.push_str(&format!(
                "{left}: {} - {}{right}",
                artist.as_deref().unwrap_or("unknown artist"),
                title.as_deref().unwrap_or("unknown title")
            ));
        }

        check_msg(msg.reply(ctx, reply).await);

        Ok(())
    })
    .await
}

#[command]
#[only_in(guilds)]
async fn remove(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let index = match args.single::<usize>() {
        Ok(index) => index,
        Err(_) => {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, "Must provide a valid index")
                    .await,
            );

            return Ok(());
        }
    };

    common_voice(ctx, msg, false, |handler| async move {
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

        check_msg(msg.channel_id.say(&ctx.http, message).await);

        Ok(())
    })
    .await
}

#[command]
#[only_in(guilds)]
async fn undeafen(ctx: &Context, msg: &Message) -> CommandResult {
    common_voice(ctx, msg, false, |handler_lock| async move {
        let mut handler = handler_lock.lock().await;
        if let Err(e) = handler.deafen(false).await {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, format!("Failed: {:?}", e))
                    .await,
            );
        }

        check_msg(msg.channel_id.say(&ctx.http, "Undeafened").await);

        Ok(())
    })
    .await
}

#[command]
#[only_in(guilds)]
async fn unmute(ctx: &Context, msg: &Message) -> CommandResult {
    common_voice(ctx, msg, false, |handler_lock| async move {
        let mut handler = handler_lock.lock().await;
        if let Err(e) = handler.mute(false).await {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, format!("Failed: {:?}", e))
                    .await,
            );
        }

        check_msg(msg.channel_id.say(&ctx.http, "Unmuted").await);
        Ok(())
    })
    .await
}

/// Checks that a message successfully sent; if not, then logs why to stdout.
fn check_msg(result: SerenityResult<Message>) {
    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }
}
