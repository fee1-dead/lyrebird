# lyrebird

An innocent discord bot that will never call `youtube-dl`

This application is dual licensed under the Apache license, version 2.0 or the MIT license, at your option.

# Features

 * Play anything that `yt-dlp` supports
 * Because it is self-hosted, the bot is yours, you can run it to keep it in voice 24/7 and there are no
limits in number of songs in queue.
 * Uses symphonia for decoding, does not need ffmpeg to work.
 * Search on YouTube and then select songs you want to enqueue

# Build instructions

First, ensure that you have the latest Rust stable installed. On distributions such as Debian and Ubuntu,
as well as on Windows, you can use [rustup] to manage and install your Rust toolchains.

## Setup the environment on Nix/NixOS

We have `shell.nix` and you can use `nix-shell` to get a environment that can compile and run the bot.
Before running `nix-shell`, you should put your bot's token into `./discord_token` as a file in the project's
root so that Nix configures the environment variable for you.


## Setup the environment on Linux

Make sure you have `gcc`, `cmake`, `libopus`, and `yt-dlp` installed. Ensure that you are using the latest
version for `yt-dlp`, otherwise downloading YouTube audio might not work correctly.

## Setting up the environment on Windows

Running the bot on windows is not tested. Feel free to open a PR or DM me on Discord (fee1-dead#7913) if
you have tried to run this on Windows

## Running the bot

Set the environment variables `BOT_OWNER_ID` and `DISCORD_TOKEN` to your Discord user ID and the token for
your bot respectively. See [this tutorial][adding bot to servers] on how to add your bot to your discord
server. Make sure that "Message Content Intent" is set as enabled in the developer portal so the bot can
read messages.

After adding your bot to your server, you need to register the slash commands that the bot supports. Using
the account that matches the `BOT_OWNER_ID`, you can run the `~register` command to either register it in
the guild specificially or globally.

# Commands

* `~register`, for use by bot owners only. For first run, use this to register slash commands on your server.
When updating the bot, use `~register` to reregister for updating the slash commands.
* `/join` - tell the bot to join your current voice channel.
* `/leave` - leaves the current vc.
* `/play <url>` - add a URL to the queue. Anything that yt-dlp supports are supported.
* `/splay <term>` - search on YouTube and add the first search result to the queue.
* `/playrange <url> <range>` For playlists, specify which songs to play. This corresponds to the `-I RANGE`
command line argument for `yt-dlp`. Values are comma-separated, ranges use `:`. Example: `1,3,5:6`. Ranges
without a lower or upper bound are also supported. (`:3` means up to the third song, and `3:` means all starting
from the third song)
* `/playall <url>` Enqueues all songs from a playlist specified at the URL
* `/playrand <url> <num>` Fetches all songs in the playlist, but take a random amount of songs from the list.
* `/search <term> [num]` Searches a given term on YouTube and returns the first `num` results. `num` defaults to
5 and cannot be greater than 25. Will include a selection menu for which songs in the result you'd like to enqueue.
* `/queue [page]` Lists current songs queued. 10 songs are displayed per page. You can specify the page in the
optional argument. By default displays the first page.
* `/shuffle` - Shuffles the queue.
* `/clear` - Stops the current song and clear all songs in the queue.
* `/rm <index>` - Removes a specific song in the queue.
* `/skip` - skips the current song and play the next one in queue.
* `/mv <from> <to>` moves a song at the index to a new index.
* `/swap <a> <b>` swaps two songs' positions in the queue.
* `/pause` pauses the current playback
* `/resume` resumes the current song
* `/deafen` and `/undeafen` - historial artifact. Planned for removal


[adding bot to servers]: https://discordjs.guide/preparations/adding-your-bot-to-servers.html
[rustup]: https://rustup.rs/