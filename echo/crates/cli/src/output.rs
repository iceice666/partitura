use echo::{EchoEventLine, Error, Event};
use serde_json::Value;
use std::io::{self, Write};

pub fn write_event(event: &Event) -> Result<(), Error> {
    write_json(&EchoEventLine::from(event))
}

pub fn write_json<T: serde::Serialize>(value: &T) -> Result<(), Error> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, value)?;
    stdout
        .write_all(b"\n")
        .map_err(|err| Error::Cli(err.to_string()))
}

pub fn write_text(delta: &str) -> Result<(), Error> {
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(delta.as_bytes())
        .map_err(|err| Error::Cli(err.to_string()))?;
    stdout.flush().map_err(|err| Error::Cli(err.to_string()))
}

pub fn write_newline() -> Result<(), Error> {
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(b"\n")
        .map_err(|err| Error::Cli(err.to_string()))
}

#[allow(dead_code)]
pub fn write_value(value: &Value) -> Result<(), Error> {
    write_json(value)
}
