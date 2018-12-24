use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::str::FromStr;

use lambda_runtime::{error::HandlerError, Context, lambda};
use rusoto_core::Region;
use rusoto_ssm::{GetParameterRequest, Ssm, SsmClient};
use serde_derive::{Deserialize, Serialize};

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

#[derive(Deserialize, Clone)]
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

    fn get_parameter(&self, name: String) -> Result<String, HandlerError> {
        let result = self.client.get_parameter(GetParameterRequest {
            name,
            with_decryption: Some(true),
        });

        match result.sync() {
            Err(err) => Err(self.context.new_error(err.description())),
            Ok(res) => Ok(res.parameter.map(|p| p.value.unwrap()).unwrap()),
        }
    }
}

fn handler(event: ApiGatewayInput, c: Context) -> Result<ApiGatewayOutput, HandlerError> {
    let ssm_facade = SsmFacade::build(&c);

    let slack_event = match serde_json::from_str(&event.body) {
        Err(err) => return Err(c.new_error(err.description())),
        Ok(ev) => ev,
    };

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
