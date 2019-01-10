use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use chrono::{Utc, Duration, TimeZone};
use hmac::Mac;
use lambda_runtime::{error::HandlerError, Context, lambda};
use serde_derive::{Deserialize, Serialize};

use awsutil::{SsmFacade, SnsFacade};
use slackutil::Channel;

type HmacSha256 = hmac::Hmac<sha2::Sha256>;

#[derive(Debug)]
enum VerificationError {
    TimestampNotFound,
    InvalidTimestamp,
    SignatureNotFound,
    InvalidVersion,
}

impl fmt::Display for VerificationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            VerificationError::TimestampNotFound => write!(f, "key not found"),
            VerificationError::InvalidTimestamp => write!(f, "invalid time stamp"),
            VerificationError::SignatureNotFound => write!(f, "signature not found"),
            VerificationError::InvalidVersion => write!(f, "invalid version"),
        }
    }
}

impl Error for VerificationError {
    fn description(&self) -> &str {
        match self {
            VerificationError::TimestampNotFound => "key not found",
            VerificationError::InvalidTimestamp => "invalid time stamp",
            VerificationError::SignatureNotFound => "signature not found",
            VerificationError::InvalidVersion => "invalid version",
        }
    }

    fn cause(&self) -> Option<&dyn Error> {
        None
    }

    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

#[derive(Deserialize, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum EventContent {
    ChannelCreated { channel: Channel },
}

#[derive(Deserialize, Clone)]
#[serde(untagged)]
#[serde(rename_all = "snake_case")]
enum SlackEvent {
    UrlVerification { challenge: String, token: String },
    NormalEvent { event: EventContent, token: String },
}

#[derive(Serialize, Clone)]
struct SlackResponse {
    challenge: Option<String>,
}

impl SlackResponse {
    fn new(challenge: Option<String>) -> Self {
        SlackResponse { challenge }
    }
}

#[derive(Deserialize, Clone, Debug)]
struct ApiGatewayInput {
    // see: https://docs.aws.amazon.com/apigateway/latest/developerguide/set-up-lambda-proxy-integrations.html#api-gateway-simple-proxy-for-lambda-input-format
    headers: HashMap<String, String>,
    body: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ApiGatewayOutput {
    // see: https://docs.aws.amazon.com/apigateway/latest/developerguide/set-up-lambda-proxy-integrations.html#api-gateway-simple-proxy-for-lambda-output-format
    status_code: u8,
    headers: HashMap<String, String>,
    body: String,
}

impl ApiGatewayOutput {
    fn new(status_code: u8, headers: HashMap<String, String>, body: String) -> Self {
        ApiGatewayOutput { status_code, headers, body }
    }
}

fn verify_request(req: &ApiGatewayInput, signing_secret: &str) -> Result<(), Box<dyn Error>> {
    // see: https://api.slack.com/docs/verifying-requests-from-slack
    let timestamp = match req.headers.get("X-Slack-Request-Timestamp") {
        None => return Err(VerificationError::TimestampNotFound.into()),
        Some(t) => t.parse::<i64>()?,
    };

    let timestamp = Utc.timestamp(timestamp, 0);
    let now = Utc::now();
    let duration = now - timestamp;
    if duration < Duration::minutes(0) || duration > Duration::minutes(5) {
        return Err(VerificationError::InvalidTimestamp.into());
    }

    let base = format!("v0:{}:{}", timestamp.timestamp(), &req.body);
    let mut mac = HmacSha256::new_varkey(signing_secret.as_bytes()).expect("invalid key length");
    mac.input(base.as_bytes());

    let signature = match req.headers.get("X-Slack-Signature") {
        None => return Err(VerificationError::SignatureNotFound.into()),
        Some(sig) => sig,
    };

    if &signature[..3] != "v0=" {
        return Err(VerificationError::InvalidVersion.into());
    }

    let signature = hex::decode(&signature[3..])?;
    mac.verify(&signature).map_err(|e| e.into())
}

fn handler(event: ApiGatewayInput, c: Context) -> Result<ApiGatewayOutput, HandlerError> {
    let ssm_facade = SsmFacade::build(&c)?;
    let signing_secret = ssm_facade.get_parameter("/slack-new-channel-notification/signing-secret")?;
    verify_request(&event, &signing_secret).map_err(|err| c.new_error(err.description()))?;

    log::info!("event = {}", event.body);
    let slack_event = serde_json::from_str(&event.body).map_err(|err| c.new_error(err.description()))?;
    let response = match slack_event {
        SlackEvent::UrlVerification { challenge, .. } => SlackResponse::new(Some(challenge)),
        SlackEvent::NormalEvent { event, .. } => {
            match event {
                EventContent::ChannelCreated { channel } => {
                    log::info!("id: {}, name: {}", channel.id, channel.name);
                    let sns_facade = SnsFacade::build(&c)?;
                    let message = serde_json::to_string(&channel).map_err(|err| c.new_error(err.description()))?;
                    sns_facade.publish(message)?;
                    SlackResponse::new(None)
                }
            }
        },
    };

    Ok(ApiGatewayOutput::new(200, HashMap::new(), serde_json::to_string(&response).unwrap()))
}

fn main() -> Result<(), Box<dyn Error>> {
    simple_logger::init_with_level(log::Level::Info)?;
    lambda!(handler);

    Ok(())
}
