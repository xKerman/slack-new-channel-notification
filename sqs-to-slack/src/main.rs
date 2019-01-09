use std::error::Error;

use lambda_runtime::{error::HandlerError, Context, lambda};
use rusoto_core::Region;
use serde_derive::{Deserialize, Serialize};

// see: https://github.com/serde-rs/serde/issues/994#issuecomment-316895860
mod json_string {
    use serde::de::{self, Deserialize, DeserializeOwned, Deserializer};

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
        where T: DeserializeOwned,
              D: Deserializer<'de>
    {
        let j = String::deserialize(deserializer)?;
        serde_json::from_str(&j).map_err(de::Error::custom)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct Channel {
    id: String,
    name: String,
    created: u64,
    creator: String,
}

#[derive(Clone, Debug, Deserialize)]
struct SqsMessageBody {
    #[serde(rename = "Message")]
    #[serde(with = "json_string")]
    message: Channel,
}

#[derive(Clone, Debug, Deserialize)]
struct SqsMessage {
    #[serde(with = "json_string")]
    body: Option<SqsMessageBody>,
}

#[derive(Clone, Deserialize)]
struct SqsEvent {
    #[serde(rename = "Records")]
    records: Vec<SqsMessage>,
}

#[derive(Clone, Serialize)]
struct Output;

fn handler(event: SqsEvent, c: Context) -> Result<Output, HandlerError> {
    log::info!("=== start handler ===");
    event.records.iter().for_each(|ev| {
        match &ev.body {
            None => return,
            Some(body) => {
                log::info!("channel = {:?}", body.message);
            }
        };
    });
    log::info!("=== end handler ===");

    Ok(Output)
}

fn main() -> Result<(), Box<dyn Error>> {
    simple_logger::init_with_level(log::Level::Info)?;
    log::info!("before handler");
    lambda!(handler);

    Ok(())
}
