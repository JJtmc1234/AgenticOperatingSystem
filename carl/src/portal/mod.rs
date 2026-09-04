//! The room where JJ, Hunter, Atlas and Carl talk, reached from this side.
//!
//! People open the room in a browser. Agents cannot, so this is their door into the same room,
//! through the same API, with the same rules. One room and one record, rather than a chat for
//! the humans and a log for the machines that nobody reads together.
//!
//! **An agent's identity comes from its password, exactly like a person's.** The server takes
//! the name from whichever stored hash matched and ignores any name in the request, so Carl
//! cannot post as Hunter and a compromised agent cannot impersonate JJ. Nobody picks their own
//! name anywhere in this system.
//!
//! The credentials live in one file outside the repository, `~/.carl/portal.json`, readable
//! only by JJ. An agent reaches the room through `carl portal`, which is a scoped command in
//! its tool list, and never holds the password itself.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

mod client;
mod render;

pub use client::{Portal, Said};
pub use render::{line_of, since, transcript};

/// Whether this address is this machine talking to itself.
///
/// Matched on the host rather than by searching the string. `https://evil.example/?x=localhost`
/// contains the word and is not loopback, and a check that only looked for the word would hand
/// somebody a password over the open internet.
fn is_loopback(api: &str) -> bool {
    let Some(rest) = api.strip_prefix("http://") else {
        return false;
    };
    // Up to the port or the path, whichever comes first.
    let host = rest
        .split(['/', ':'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    host == "localhost" || host == "127.0.0.1" || host == "[::1]" || host == "::1"
}

/// Where the credentials live. Outside any repository on purpose.
pub fn config_path(home: &Path) -> PathBuf {
    home.join("portal.json")
}

/// What `carl portal` needs to reach the room.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// The Worker's address, for example `https://me-portal.<subdomain>.workers.dev`.
    pub api: String,
    /// This machine's password. It decides which name the messages carry.
    pub password: String,
    /// The last message id this machine has seen, so `carl portal read` can show what is new.
    #[serde(default)]
    pub seen: i64,
}

impl Config {
    pub fn load(home: &Path) -> Result<Self> {
        let path = config_path(home);
        let text = std::fs::read_to_string(&path).map_err(|e| {
            Error::Refused(format!(
                "no portal credentials at {} ({e}).\n\nWrite that file as:\n  {{\n    \"api\": \
                 \"https://me-portal.<subdomain>.workers.dev\",\n    \"password\": \"the one for \
                 this machine\"\n  }}\n\nThe password decides which name your messages carry, so \
                 use Carl's rather than JJ's.",
                path.display()
            ))
        })?;
        let config: Config = serde_json::from_str(&text)
            .map_err(|e| Error::Refused(format!("{} is not valid JSON: {e}", path.display())))?;

        if config.api.trim().is_empty() || config.password.trim().is_empty() {
            return Err(Error::Refused(format!(
                "{} is missing the api address or the password",
                path.display()
            )));
        }
        // Refused rather than warned. A password sent over plain http crosses the network in the
        // clear, and this one is an identity in a room with JJ's mentor in it.
        //
        // Loopback is the one exception, and it is an exception because nothing leaves the
        // machine: there is no network for the password to cross. That is what lets the room be
        // run locally with `wrangler dev` before anybody has a hosting account, which is the
        // difference between agents talking to each other today and waiting on one.
        if !config.api.starts_with("https://") && !is_loopback(&config.api) {
            return Err(Error::Refused(format!(
                "the portal address must be https, and {} is not. The one exception is \
                 http://localhost or http://127.0.0.1, where nothing leaves this machine",
                config.api
            )));
        }
        Ok(config)
    }

    /// Writes back only the watermark. The password is never rewritten by this program, so a bug
    /// here cannot lose the one thing that is not recoverable from anywhere else.
    pub fn remember_seen(&self, home: &Path, seen: i64) -> Result<()> {
        let path = config_path(home);
        let mut current = Self::load(home)?;
        current.seen = seen;
        let text = serde_json::to_string_pretty(&current)?;
        std::fs::write(&path, text)?;
        Ok(())
    }
}

#[cfg(test)]
mod client_tests;
#[cfg(test)]
mod tests;
