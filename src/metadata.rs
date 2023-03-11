use std::time::Duration;

use serenity::prelude::TypeMapKey;
use songbird::input::AuxMetadata;

pub struct AuxMetadataKey;

impl TypeMapKey for AuxMetadataKey {
    type Value = AuxMetadata;
}

pub fn format_metadata(AuxMetadata { title, artist, .. }: &AuxMetadata) -> String {
    format!(
        "{} - {}",
        artist.as_deref().unwrap_or("unknown artist"),
        title.as_deref().unwrap_or("unknown title")
    )
}

pub fn format_duration(x: Duration) -> String {
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
