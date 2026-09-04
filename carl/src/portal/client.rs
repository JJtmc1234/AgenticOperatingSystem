//! Talking to the room over HTTP.
//!
//! Two calls and nothing else. `say` puts a message in the room, `read` takes back everything
//! after an id. There is no delete, no edit and no way to fetch one message on its own, because
//! a record people trust is one nobody can quietly change and the room is small enough that
//! reading from a watermark is the whole of what anybody needs.
//!
//! The password goes in the `Authorization` header rather than the body. A body is what gets
//! logged when somebody debugs a request.

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// One message in the room.
///
/// `who` is what the server decided from the password, never what the sender asked to be
/// called. It is on the way in as well as the way out so a transcript can be rendered from
/// what was received rather than from what was requested.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Said {
    pub id: i64,
    pub who: String,
    pub text: String,
    /// Seconds since the epoch, as the server saw it. The sender's clock is not consulted,
    /// because an agent with a wrong clock would otherwise reorder the room.
    pub at: i64,
}

/// The room, reached from this side.
pub struct Portal {
    api: String,
    password: String,
    agent: ureq::Agent,
}

impl Portal {
    pub fn new(api: &str, password: &str) -> Self {
        Self {
            api: api.trim_end_matches('/').to_string(),
            password: password.to_string(),
            agent: ureq::Agent::new_with_defaults(),
        }
    }

    /// Puts a message in the room and hands back what the server recorded.
    ///
    /// What comes back is the message as stored, including the name the password earned. A
    /// caller that printed what it sent rather than what was returned would show Carl's own
    /// idea of who he is, which is exactly the thing this design does not let him choose.
    pub fn say(&self, text: &str) -> Result<Said> {
        let text = text.trim();
        if text.is_empty() {
            return Err(Error::Refused(
                "nothing to say. An empty message in a shared room is noise with a name on it"
                    .into(),
            ));
        }
        let mut res = self
            .agent
            .post(format!("{}/say", self.api))
            .header("Authorization", &format!("Bearer {}", self.password))
            .header("Content-Type", "application/json; charset=utf-8")
            .send_json(serde_json::json!({ "text": text }))
            .map_err(|e| Self::explain("say", e))?;

        res.body_mut().read_json::<Said>().map_err(|e| {
            Error::Refused(format!(
                "the room accepted the message but its answer was not one message: {e}"
            ))
        })
    }

    /// Everything said after `after`. Pass 0 for the whole room.
    pub fn read(&self, after: i64) -> Result<Vec<Said>> {
        let mut res = self
            .agent
            .get(format!("{}/read?after={after}", self.api))
            .header("Authorization", &format!("Bearer {}", self.password))
            .call()
            .map_err(|e| Self::explain("read", e))?;

        res.body_mut().read_json::<Vec<Said>>().map_err(|e| {
            Error::Refused(format!("the room's answer was not a list of messages: {e}"))
        })
    }

    /// One message for the common failures, rather than the transport's own wording.
    ///
    /// A 401 here is not a network problem and telling somebody "http status 401" sends them
    /// looking at the wrong thing. It means the password in `~/.carl/portal.json` is not one
    /// the room knows, and that is what it should say.
    fn explain(what: &str, e: ureq::Error) -> Error {
        if let ureq::Error::StatusCode(code) = e {
            let why = match code {
                401 | 403 => {
                    "the room did not recognise that password. It is the one line in \
                     ~/.carl/portal.json that decides which name your messages carry, so a wrong \
                     one is an identity problem rather than a typo"
                }
                404 => "there is no room at that address. Check `api` in ~/.carl/portal.json",
                429 => "the room is rate limiting. Wait and try again",
                _ => "the room refused",
            };
            return Error::Refused(format!("portal {what}: {why} (http {code})"));
        }
        Error::Refused(format!("portal {what} could not reach the room: {e}"))
    }
}
