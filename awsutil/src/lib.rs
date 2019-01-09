use std::env;
use std::error::Error;
use std::default::Default;
use std::str::FromStr;

use lambda_runtime::{error::HandlerError, Context};
use rusoto_core::Region;
use rusoto_sns::{Sns, SnsClient, PublishInput};
use rusoto_ssm::{GetParameterRequest, Ssm, SsmClient};

pub struct SsmFacade<'a> {
    context: &'a Context,
    client: SsmClient,
}

impl<'a> SsmFacade<'a> {
    pub fn build(context: &'a Context) -> Result<Self, HandlerError> {
        let region = match env::var("AWS_REGION") {
            Ok(region) => Region::from_str(region.as_str()).unwrap(),
            Err(err) => return Err(context.new_error(err.description())),
        };
        let client = SsmClient::new(region);

        Ok(SsmFacade { context, client })
    }

    pub fn get_parameter(&self, name: &str) -> Result<String, HandlerError> {
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

pub struct SnsFacade<'a> {
    context: &'a Context,
    client: SnsClient,
}

impl<'a> SnsFacade<'a> {
    pub fn build(context: &'a Context) -> Result<Self, HandlerError> {
        let region = match env::var("AWS_REGION") {
            Ok(region) => Region::from_str(region.as_str()).unwrap(),
            Err(err) => return Err(context.new_error(err.description())),
        };
        let client = SnsClient::new(region);

        Ok(SnsFacade {
            context,
            client,
        })
    }

    pub fn publish(&self, message: String) -> Result<(), HandlerError> {
        let topic_arn = env::var("AWS_SNS_TOPIC_ARN").map_err(|err| self.context.new_error(err.description()))?;

        let result = self.client.publish(PublishInput {
            message,
            topic_arn: Some(topic_arn),
            ..Default::default()
        });

        match result.sync() {
            Ok(_) => Ok(()),
            Err(err) => Err(self.context.new_error(err.description())),
        }
    }
}
