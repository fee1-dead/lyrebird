use songbird::input::{Input, YoutubeDl};

use crate::metadata::{format_metadata, AuxMetadataKey};
use crate::{CommandResult, Context};

use crate::vc::enter_vc;

crate::commands!(play, splay);

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

        let input = mk(c, term);
        let mut source: Input = input.into();
        let metadata = source.aux_metadata().await?;
        let msg = format!("Queued: {}", format_metadata(&metadata),);
        let handle = handler.enqueue_input(source).await;
        handle
            .typemap()
            .write()
            .await
            .insert::<AuxMetadataKey>(metadata);
        ctx.say(msg).await?;
        Ok(())
    })
    .await
}
