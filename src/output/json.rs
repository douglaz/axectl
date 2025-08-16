use anyhow::Result;
use serde::Serialize;
use serde_json;

pub trait JsonOutput {
    fn to_json(&self, pretty: bool) -> Result<String>;
}

impl<T: Serialize> JsonOutput for T {
    fn to_json(&self, pretty: bool) -> Result<String> {
        if pretty {
            Ok(serde_json::to_string_pretty(self)?)
        } else {
            Ok(serde_json::to_string(self)?)
        }
    }
}

pub fn print_json<T: Serialize>(data: &T, pretty: bool) -> Result<()> {
    println!("{}", data.to_json(pretty)?);
    Ok(())
}

pub fn format_json<T: Serialize>(data: &T, pretty: bool) -> Result<String> {
    data.to_json(pretty)
}