use std::error::Error;

use chrono::TimeZone;
use lambda_runtime::{error::HandlerError, Context, lambda};
use serde_derive::{Deserialize, Serialize};

use awsutil::SsmFacade;

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
    created: i64,
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
    let ssm_facade = SsmFacade::build(&c)?;
    let webhook_url = ssm_facade.get_parameter("/slack-new-channel-notification/slack-webhook-url")?;
    for ev in event.records {
        match &ev.body {
            None => continue,
            Some(body) => {
                log::info!("channel = {:?}", body.message);
                let channel = &body.message;
                let created = chrono::Utc.timestamp(channel.created, 0).with_timezone(&chrono::FixedOffset::east(9 * 3600));
                let texts = vec![
                    slack_hook::SlackTextContent::Link(slack_hook::SlackLink::new(&format!("#{}", &channel.id), &channel.name)),
                    slack_hook::SlackTextContent::Text(format!(", at {}", created).into()),
                ];
                let payload = slack_hook::PayloadBuilder::new()
                    .username("Slack New Channel")
                    .icon_emoji(":new_moon_with_face:")
                    .text(texts.as_slice())
                    .build()
                    .unwrap();
                let slack = slack_hook::Slack::new(webhook_url.as_str()).unwrap();
                slack.send(&payload).map_err(|err| c.new_error(err.description()))?;
            }
        };
    }
    log::info!("=== end handler ===");

    Ok(Output)
}

fn main() -> Result<(), Box<dyn Error>> {
    simple_logger::init_with_level(log::Level::Info)?;
    log::info!("before handler");
    lambda!(handler);

    Ok(())
}
