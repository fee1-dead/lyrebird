use std::env;
use std::future::Future;
use std::sync::Arc;

use serenity::prelude::Mutex;
// This trait adds the `register_songbird` and `register_songbird_with` methods
// to the client builder below, making it easy to install this voice client.
// The voice client can be retrieved in any command using `songbird::get(ctx).await`.
use songbird::{SerenityInit, Call};

// Import the `Context` to handle commands.
use serenity::client::Context;

use serenity::{
    async_trait,
    client::{Client, EventHandler},
    framework::{
        StandardFramework,
        standard::{
            Args, CommandResult,
            macros::{command, group},
        },
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
#[commands(deafen, join, leave, splay, play, ping, undeafen, unmute, skip, queue, remove)]
struct General;

fn main() {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(main_inner());
}

async fn main_inner() {
    tracing_subscriber::fmt::init();
    
    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN")
        .expect("Expected a token in the environment");

    let framework = StandardFramework::new()
        .configure(|c| c
                   .prefix("~"))
        .group(&GENERAL_GROUP);

    let intents = GatewayIntents::non_privileged()
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .framework(framework)
        .register_songbird()
        .await
        .expect("Err creating client");

    tokio::spawn(async move {
        let _ = client.start().await.map_err(|why| println!("Client ended: {:?}", why));
    });
    
    let _ = tokio::signal::ctrl_c().await;
    println!("Received Ctrl-C, shutting down.");
}


async fn common_voice<F: FnOnce(Arc<Mutex<Call>>) -> T, T: Future<Output = CommandResult>>(ctx: &Context, msg: &Message, f: F) -> CommandResult {
    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx).await
        .expect("Songbird Voice client placed in at initialisation.").clone();

    let handler_lock = match manager.get(guild_id) {
        Some(handler) => handler,
        None => {
            check_msg(msg.reply(ctx, "Not in a voice channel").await);

            return Ok(());
        },
    };

    f(handler_lock).await
}

#[command]
#[only_in(guilds)]
async fn deafen(ctx: &Context, msg: &Message) -> CommandResult {
    common_voice(ctx, msg, |handler| async move {
        let mut handler = handler.lock().await;
        if handler.is_deaf() {
            check_msg(msg.channel_id.say(&ctx.http, "Already deafened").await);
        } else {
            if let Err(e) = handler.deafen(true).await {
                check_msg(msg.channel_id.say(&ctx.http, format!("Failed: {:?}", e)).await);
            }
    
            check_msg(msg.channel_id.say(&ctx.http, "Deafened").await);
        }
    
        Ok(())
    }).await
}

#[command]
#[only_in(guilds)]
async fn join(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let channel_id = guild
        .voice_states.get(&msg.author.id)
        .and_then(|voice_state| voice_state.channel_id);

    let connect_to = match channel_id {
        Some(channel) => channel,
        None => {
            check_msg(msg.reply(ctx, "Not in a voice channel").await);

            return Ok(());
        }
    };

    let manager = songbird::get(ctx).await
        .expect("Songbird Voice client placed in at initialisation.").clone();

    let _handler = manager.join(guild_id, connect_to).await;

    Ok(())
}

#[command]
#[only_in(guilds)]
async fn leave(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx).await
        .expect("Songbird Voice client placed in at initialisation.").clone();
    let has_handler = manager.get(guild_id).is_some();

    if has_handler {
        if let Err(e) = manager.remove(guild_id).await {
            check_msg(msg.channel_id.say(&ctx.http, format!("Failed: {:?}", e)).await);
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
    common_voice(ctx, msg, |handler_lock| async move {
        let handler = handler_lock.lock().await;
        if handler.queue().is_empty() {
            check_msg(msg.channel_id.say(ctx, "already skipped").await);
        } else {
            let _ = handler.queue().skip();
            check_msg(msg.channel_id.say(&ctx.http, "skipped song").await);
        }
        Ok(())
    }).await
}

#[command]
#[only_in(guilds)]
async fn splay(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let term = match args.single::<String>() {
        Ok(term) => term,
        Err(_) => {
            check_msg(msg.channel_id.say(&ctx.http, "Must provide a term to search for").await);

            return Ok(());
        },
    };


    common_voice(ctx, msg, |handler_lock| async move {
        let mut handler = handler_lock.lock().await;

        let input = match Restartable::ytdl_search(term, true).await {
            Ok(input) => input,
            Err(why) => {
                println!("Err starting source: {:?}", why);

                check_msg(msg.channel_id.say(&ctx.http, "Error sourcing ffmpeg").await);

                return Ok(());
            },
        };
        handler.enqueue_source(input.into());
        check_msg(msg.channel_id.say(&ctx.http, "Added to queue").await);
        Ok(())
    }).await
}

#[command]
#[only_in(guilds)]
async fn play(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let url = match args.single::<String>() {
        Ok(url) => url,
        Err(_) => {
            check_msg(msg.channel_id.say(&ctx.http, "Must provide a URL to a video or audio").await);

            return Ok(());
        },
    };

    if !url.starts_with("http") {
        check_msg(msg.channel_id.say(&ctx.http, "Must provide a valid URL").await);

        return Ok(());
    }

    common_voice(ctx, msg, |handler_lock| async move {
        let mut handler = handler_lock.lock().await;

        let input = match Restartable::ytdl(url, true).await {
            Ok(input) => input,
            Err(why) => {
                println!("Err starting source: {:?}", why);

                check_msg(msg.channel_id.say(&ctx.http, "Error sourcing ffmpeg").await);

                return Ok(());
            },
        };
        handler.enqueue_source(input.into());
        check_msg(msg.channel_id.say(&ctx.http, "Added to queue").await);
        Ok(())
    }).await
}

#[command]
#[only_in(guilds)]
async fn queue(ctx: &Context, msg: &Message) -> CommandResult {
    common_voice(ctx, msg, |handler| async move {
        let handler = handler.lock().await;

        let mut reply = String::new();
        for (n, song) in handler.queue().current_queue().into_iter().enumerate() {
            let Metadata { title, artist, .. } = song.metadata();
            if !reply.is_empty() {
                reply.push('\n');
            }
            reply.push_str(&format!("{n}: {} - {}", artist.as_deref().unwrap_or("unknown artist"), title.as_deref().unwrap_or("unknown title")));
        }
    
        check_msg(msg.reply(ctx, reply).await);

        Ok(())
    }).await
}

#[command]
#[only_in(guilds)]
async fn remove(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let index = match args.single::<usize>() {
        Ok(index) => index,
        Err(_) => {
            check_msg(msg.channel_id.say(&ctx.http, "Must provide a valid index").await);

            return Ok(());
        },
    };

    common_voice(ctx, msg, |handler| async move {
        let handler = handler.lock().await;

        let message = handler.queue().modify_queue(|x| {
            if let Some(track) = x.remove(index) {
                if let Err(e) = track.stop() {
                    format!("Failed to stop track: {:?}", e)
                } else {
                    format!("Removed track: {}", track.metadata().title.as_deref().unwrap_or("unknown title"))
                }
            } else {
                format!("No track at index {}", index)
            }
        });

        check_msg(msg.channel_id.say(&ctx.http, message).await);

        Ok(())
    }).await
}

#[command]
#[only_in(guilds)]
async fn undeafen(ctx: &Context, msg: &Message) -> CommandResult {
    common_voice(ctx, msg, |handler_lock| async move {
        let mut handler = handler_lock.lock().await;
        if let Err(e) = handler.deafen(false).await {
            check_msg(msg.channel_id.say(&ctx.http, format!("Failed: {:?}", e)).await);
        }

        check_msg(msg.channel_id.say(&ctx.http, "Undeafened").await);

        Ok(())
    }).await
}

#[command]
#[only_in(guilds)]
async fn unmute(ctx: &Context, msg: &Message) -> CommandResult {
    common_voice(ctx, msg, |handler_lock| async move {
        let mut handler = handler_lock.lock().await;
        if let Err(e) = handler.mute(false).await {
            check_msg(msg.channel_id.say(&ctx.http, format!("Failed: {:?}", e)).await);
        }

        check_msg(msg.channel_id.say(&ctx.http, "Unmuted").await);
        Ok(())
    }).await
}

/// Checks that a message successfully sent; if not, then logs why to stdout.
fn check_msg(result: SerenityResult<Message>) {
    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }
}
