use std::fs;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use serde::Deserialize;

#[derive(Deserialize)]
struct Config {
    mode: String,
    owner_id: u64,
    debug: Option<Profile>,
    release: Option<Profile>,
}

impl Config {
    fn profile(&self) -> &Profile {
        match &*self.mode {
            "debug" => self.debug.as_ref().unwrap(),
            "release" => self.release.as_ref().unwrap(),
            _ => panic!("Invalid mode"),
        }
    }

    fn path(&self) -> String {
        self.profile()
            .binary_path
            .clone()
            .unwrap_or(format!("./target/{}/lyrebird", self.mode))
    }

    fn mk_command(&self) -> Command {
        let mut c = Command::new(self.path());
        c.env("DISCORD_TOKEN", &self.profile().token)
            .env("BOT_OWNER_ID", &self.owner_id.to_string())
            .env("IS_RUN_BY_RUNNER", "1")
            .stdout(Stdio::piped());
        c
    }
}

#[derive(Deserialize)]
struct Profile {
    token: String,
    binary_path: Option<String>,
}

fn main() -> color_eyre::Result<()> {
    let config: Config = toml::from_str(&fs::read_to_string("./config.toml")?)?;

    let mut command = config.mk_command();

    'x: loop {
        let mut child = command.spawn()?;

        let stdout = child.stdout.take().unwrap();
        let buf = BufReader::new(stdout);
        for l in buf.lines() {
            let l = l?;
            if let Some(path) = l.strip_prefix("!restart,path=") {
                child.kill()?;
                command = config.mk_command();
                command.env("RESTART_RECOVER_PATH", path);
                continue 'x;
            } else {
                println!("{}", l);
            }
        }

        break;
    }

    Ok(())
}
