use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fmt;
use std::str::FromStr;

use chrono::{Utc, Duration, TimeZone};
use hmac::Mac;
use lambda_runtime::{error::HandlerError, Context, lambda};
use rusoto_core::Region;
use rusoto_ssm::{GetParameterRequest, Ssm, SsmClient};
use serde_derive::{Deserialize, Serialize};

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
#[serde(rename_all = "snake_case")]
struct Channel {
    id: String,
    name: String,
    created: u8,
    creator: String,
}

#[derive(Deserialize, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum SlackEvent {
    UrlVerification { challenge: String, token: String },
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

struct SsmFacade<'a> {
    context: &'a Context,
    client: SsmClient,
}

impl<'a> SsmFacade<'a> {
    fn build(context: &'a Context) -> Result<Self, HandlerError> {
        let region = match env::var("AWS_REGION") {
            Ok(region) => Region::from_str(region.as_str()).unwrap(),
            Err(err) => return Err(context.new_error(err.description())),
        };
        let client = SsmClient::new(region);

        Ok(SsmFacade { context, client })
    }

    fn get_parameter(&self, name: &str) -> Result<String, HandlerError> {
        let result = self.client.get_parameter(GetParameterRequest {
            name: name.to_string(),
            with_decryption: Some(true),
        });

        match result.sync() {
            Err(err) => Err(self.context.new_error(err.description())),
            Ok(res) => Ok(res.parameter.map(|p| p.value.unwrap()).unwrap()),
        }
    }
}

fn verify_request(req: &ApiGatewayInput, signing_secret: &str) -> Result<(), Box<dyn Error>> {
    // see: https://api.slack.com/docs/verifying-requests-from-slack
    let timestamp = match req.headers.get("X-Slack-Request-Timestamp") {
        None => return Err(VerificationError::TimestampNotFound.into()),
        Some(t) => t.parse::<i64>()?,
    };
    log::info!("timestamp = {}", timestamp);

    let timestamp = Utc.timestamp(timestamp, 0);
    let now = Utc::now();
    let duration = now - timestamp;
    if duration < Duration::minutes(0) || duration > Duration::minutes(5) {
        return Err(VerificationError::InvalidTimestamp.into());
    }
    log::info!("timestamp check succeeded!");

    let base = format!("v0:{}:{}", timestamp.timestamp(), &req.body);
    let mut mac = HmacSha256::new_varkey(signing_secret.as_bytes()).expect("invalid key length");
    mac.input(base.as_bytes());

    let signature = match req.headers.get("X-Slack-Signature") {
        None => return Err(VerificationError::SignatureNotFound.into()),
        Some(sig) => sig,
    };
    log::info!("signature = {}", signature);

    if &signature[..3] != "v0=" {
        return Err(VerificationError::InvalidVersion.into());
    }
    log::info!("version check succeded");

    let signature = hex::decode(&signature[3..])?;
    mac.verify(&signature).map_err(|e| e.into())
}

fn handler(event: ApiGatewayInput, c: Context) -> Result<ApiGatewayOutput, HandlerError> {
    log::info!("start!!");
    log::info!("request = {:?}", event);
    let ssm_facade = SsmFacade::build(&c)?;
    log::info!("ssm facade created");
    let signing_secret = ssm_facade.get_parameter("/slack-new-channel-notification/signing-secret")?;
    log::info!("sining secret from ssm parameter store");
    verify_request(&event, &signing_secret).map_err(|err| c.new_error(err.description()))?;
    log::info!("verification succeeded!!");

    log::info!("event = {}", &event.body);
    let slack_event = serde_json::from_str(&event.body).map_err(|err| c.new_error(err.description()))?;
    let response = match slack_event {
        SlackEvent::UrlVerification { challenge, .. } => SlackResponse::new(Some(challenge)),
        _ => SlackResponse::new(None),
    };

    Ok(ApiGatewayOutput::new(200, HashMap::new(), serde_json::to_string(&response).unwrap()))
}

fn main() -> Result<(), Box<dyn Error>> {
    simple_logger::init_with_level(log::Level::Info)?;
    lambda!(handler);

    Ok(())
}
