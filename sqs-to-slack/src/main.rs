use std::error::Error;

use lambda_runtime::{error::HandlerError, Context, lambda};
use rusoto_core::Region;
use serde_derive::{Deserialize, Serialize};

#[derive(Clone, Deserialize)]
struct SqsMessage {
    body: Option<String>,
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
        log::info!("body = {:?}", ev.body);
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
