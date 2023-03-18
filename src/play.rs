use std::process::Stdio;

use poise::{CreateReply, ReplyHandle};
use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use songbird::input::{Input, YoutubeDl};
use tokio::process::Command;

use crate::metadata::{format_metadata, AuxMetadataKey};
use crate::{CommandResult, Context, Error};

use crate::vc::enter_vc;

crate::commands!(play, splay, playall, playrand, playrange);

#[poise::command(slash_command)]
/// Add a song to queue from the given URL.
pub async fn play(
    ctx: Context<'_>,
    #[description = "URL of song to play"] url: String,
) -> CommandResult {
    play_common(
        ctx,
        url,
        |h, url| YoutubeDl::new(h.data().client.clone(), url).into(),
        false,
    )
    .await
}

#[poise::command(slash_command)]
pub async fn splay(
    ctx: Context<'_>,
    #[rest]
    #[description = "keyword to search for"]
    keyword: String,
) -> CommandResult {
    play_common(
        ctx,
        keyword,
        |h, term| YoutubeDl::new(h.data().client.clone(), format!("ytsearch1:{term}")).into(),
        false,
    )
    .await
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Output {
    pub url: String,
    ie_key: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub channel: Option<String>,
    _type: String,
}

impl Output {
    pub fn is_playable(&self) -> bool {
        self._type == "url" && !self.is_playlist()
    }
    pub fn is_playlist(&self) -> bool {
        self.ie_key == "YoutubePlaylist" || self.ie_key == "YoutubeTab"
    }
}

pub async fn play_multiple(
    ctx: Context<'_>,
    input: Vec<Input>,
    handler: &mut songbird::Call,
) -> CommandResult {
    let mut cnt = 0usize;
    let mut msg = None;
    for input in input {
        msg = Some(play_inner(ctx, input, handler, msg).await?);
        cnt += 1;
    }
    if cnt > 1 {
        maybe_edit(ctx, msg, format!("Queued {cnt} songs")).await?;
    }
    Ok(())
}

#[poise::command(slash_command)]
/// Play all songs from a given playlist
pub async fn playall(
    ctx: Context<'_>,
    #[description = "url of playlist"] url: String,
) -> CommandResult {
    ctx.defer().await?;
    let cmd = Command::new("yt-dlp")
        .arg("--flat-playlist")
        .arg("-s")
        .arg("-j")
        .arg(url)
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .output()
        .await?;

    // TODO use exit_ok

    let s = String::from_utf8(cmd.stdout)?;

    enter_vc(ctx, true, |handler, ctx| async move {
        let parsed = s
            .lines()
            .map(|x| serde_json::from_str::<Output>(x))
            .collect::<Result<Vec<_>, _>>()?;
        let inputs = parsed
            .into_iter()
            .filter(Output::is_playable)
            .map(|x| YoutubeDl::new(ctx.data().client.clone(), x.url).into())
            .collect::<Vec<_>>();
        play_multiple(ctx, inputs, &mut *handler.lock().await).await?;
        Ok(())
    })
    .await?;

    Ok(())
}

#[poise::command(slash_command)]
/// Play random songs from a given playlist
pub async fn playrand(
    ctx: Context<'_>,
    #[description = "url of the playlist"] url: String,
    #[description = "number of songs to play"] num: usize,
) -> CommandResult {
    ctx.defer().await?;
    let cmd = Command::new("yt-dlp")
        .arg("--flat-playlist")
        .arg("-s")
        .arg("-j")
        .arg(url)
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .output()
        .await?;
    let s = String::from_utf8(cmd.stdout)?;
    let outputs = s
        .lines()
        .map(|l| serde_json::from_str::<Output>(l))
        .filter_map(|x| x.ok().filter(|x| !x.is_playlist()))
        .collect::<Vec<_>>();
    let chooser = outputs
        .choose_multiple(&mut thread_rng(), num)
        .cloned()
        .collect::<Vec<_>>();
    drop(outputs);
    enter_vc(ctx, true, |handler, ctx| async move {
        play_multiple(
            ctx,
            chooser
                .into_iter()
                .map(|x| YoutubeDl::new(ctx.data().client.clone(), x.url).into())
                .collect(),
            &mut *handler.lock().await,
        )
        .await
    })
    .await
}

#[poise::command(slash_command)]
/// Play a range of songs from a playlist
pub async fn playrange(
    ctx: Context<'_>,
    #[description = "url of the playlist"] url: String,
    #[description = "range"] range: String,
) -> CommandResult {
    ctx.defer().await?;
    let cmd = Command::new("yt-dlp")
        .arg("--flat-playlist")
        .arg("-s")
        .arg("-j")
        .arg("-I")
        .arg(range)
        .arg(url)
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .output()
        .await?;
    let s = String::from_utf8(cmd.stdout)?;
    let outputs = s
        .lines()
        .map(serde_json::from_str::<Output>)
        .collect::<Result<Vec<_>, _>>()?;
    let outputs = outputs
        .into_iter()
        .filter(|x| !x.is_playlist())
        .collect::<Vec<_>>();
    enter_vc(ctx, true, |handler, ctx| async move {
        play_multiple(
            ctx,
            outputs
                .into_iter()
                .map(|x| YoutubeDl::new(ctx.data().client.clone(), x.url).into())
                .collect(),
            &mut *handler.lock().await,
        )
        .await
    })
    .await
}

async fn maybe_edit<'a>(
    ctx: Context<'a>,
    prev: Option<ReplyHandle<'a>>,
    msg: String,
) -> Result<ReplyHandle<'a>, Error> {
    if let Some(m) = prev {
        m.edit(ctx, CreateReply::new().content(msg)).await?;
        Ok(m)
    } else {
        Ok(ctx.say(msg).await?)
    }
}

async fn play_inner<'a>(
    ctx: Context<'a>,
    mut input: Input,
    handler: &mut songbird::Call,
    edit: Option<ReplyHandle<'a>>,
) -> Result<ReplyHandle<'a>, Error> {
    let metadata = input.aux_metadata().await?;
    let msg = format!("Queued: {}", format_metadata(&metadata));
    let handle = handler.enqueue_input(input).await;
    handle
        .typemap()
        .write()
        .await
        .insert::<AuxMetadataKey>(metadata);

    maybe_edit(ctx, edit, msg).await
}

async fn play_common(
    ctx: Context<'_>,
    term: String,
    mk: fn(Context<'_>, String) -> Input,
    url: bool,
) -> CommandResult {
    ctx.defer().await?;
    if url && !term.starts_with("http") {
        ctx.say("Argument must be a valid URL").await?;
        return Ok(());
    }
    enter_vc(ctx, true, |handler_lock, c| async move {
        let mut handler = handler_lock.lock().await;
        play_inner(c, mk(c, term), &mut handler, None).await?;
        Ok(())
    })
    .await
}
