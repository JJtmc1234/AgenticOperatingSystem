//! JSON-RPC 2.0, which is what MCP is carried in.
//!
//! One JSON object per line on stdin, one per line on stdout. Nothing else may ever be
//! written to stdout, because anything that is becomes a parse error at the other end. That
//! is why every diagnostic in this crate goes to stderr, and why a capability server is a
//! bad place to put a `println!`.
//!
//! Two rules from the specification that are easy to get wrong and silent when you do.
//!
//! A request has an id and must be answered. A notification has no id and must never be
//! answered, and replying to one is a protocol error at the client.
//!
//! An error is not a failed call. `tools/call` returning a tool error still succeeds at the
//! JSON-RPC level, with `isError` set inside the result, so the model can see what went wrong
//! and try something else. A JSON-RPC error means the request itself was unusable, and the
//! model never sees it. Reporting a refused file as a protocol error hides it from the one
//! participant who could do something about it.

use serde_json::{Value, json};

/// One incoming message.
#[derive(Debug, Clone, PartialEq)]
pub struct Incoming {
    pub method: String,
    pub params: Value,
    /// Absent for a notification, which must not be answered.
    pub id: Option<Value>,
}

impl Incoming {
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }
}

/// Reads one line. `None` for anything that is not a usable message.
pub fn parse(line: &str) -> Option<Incoming> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    let method = v.get("method")?.as_str()?.to_string();
    Some(Incoming {
        method,
        params: v.get("params").cloned().unwrap_or_else(|| json!({})),
        // Deliberately kept as a Value. The specification allows a string or a number and
        // says to echo back exactly what was sent, so converting it would corrupt it.
        id: v.get("id").cloned(),
    })
}

pub fn result(id: &Value, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

pub fn error(id: &Value, code: i64, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
    .to_string()
}

/// A tool that ran and failed, which is a successful call carrying bad news.
///
/// The model sees this and can act on it. A JSON-RPC error would be swallowed by the client
/// and the model would only know that nothing happened.
pub fn tool_error(text: impl Into<String>) -> Value {
    json!({
        "content": [{ "type": "text", "text": text.into() }],
        "isError": true
    })
}

/// A tool that ran and worked.
pub fn tool_ok(text: impl Into<String>) -> Value {
    json!({
        "content": [{ "type": "text", "text": text.into() }],
        "isError": false
    })
}

pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_carries_its_id() {
        let m = parse(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#).unwrap();
        assert_eq!(m.method, "tools/list");
        assert!(!m.is_notification());
        assert_eq!(m.id, Some(json!(1)));
    }

    /// Answering a notification is a protocol error at the client, and notifications are
    /// common, so getting this wrong breaks the connection rather than one call.
    #[test]
    fn a_notification_has_no_id_and_must_not_be_answered() {
        let m = parse(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).unwrap();
        assert!(m.is_notification());
    }

    /// The specification allows either and says to echo back what was sent, so the id is
    /// carried as it arrived rather than parsed into a number.
    #[test]
    fn an_id_can_be_a_string_or_a_number() {
        for (raw, want) in [
            (r#"{"id":7,"method":"x"}"#, json!(7)),
            (r#"{"id":"abc","method":"x"}"#, json!("abc")),
        ] {
            assert_eq!(parse(raw).unwrap().id, Some(want));
        }
        let out = result(&json!("abc"), json!({}));
        assert!(out.contains(r#""id":"abc""#), "{out}");
    }

    #[test]
    fn missing_params_become_an_empty_object() {
        assert_eq!(parse(r#"{"id":1,"method":"x"}"#).unwrap().params, json!({}));
    }

    #[test]
    fn rubbish_is_not_a_message() {
        for bad in ["", "not json", "{}", r#"{"id":1}"#, "[1,2,3]"] {
            assert_eq!(parse(bad), None, "should have rejected {bad:?}");
        }
    }

    /// The distinction that matters. A refused file is news for the model, not a broken
    /// request, and sending it as a protocol error hides it from the only participant who
    /// could do something about it.
    #[test]
    fn a_failed_tool_is_a_successful_call() {
        let e = tool_error("that path is outside the root");
        assert_eq!(e["isError"], json!(true));
        assert!(e["content"][0]["text"].as_str().unwrap().contains("root"));

        let wrapped = result(&json!(1), e);
        assert!(wrapped.contains(r#""result""#), "{wrapped}");
        assert!(!wrapped.contains(r#""error""#), "{wrapped}");
    }

    #[test]
    fn a_protocol_error_is_shaped_differently() {
        let out = error(&json!(1), METHOD_NOT_FOUND, "no such method");
        assert!(out.contains(r#""error""#));
        assert!(out.contains("-32601"));
    }
}
