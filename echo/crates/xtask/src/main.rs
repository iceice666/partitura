use clap::{Parser, Subcommand};
use std::{fs, path::PathBuf};

#[derive(Debug, Parser)]
#[command(name = "echo-xtask")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    RegenerateModels {
        #[arg(long, default_value = "model-registry/source.json")]
        source: PathBuf,
        #[arg(long, default_value = "model-registry/snapshot.json")]
        output: PathBuf,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match Cli::parse().command {
        Command::RegenerateModels { source, output } => {
            let body = fs::read_to_string(source)?;
            let value: serde_json::Value = serde_json::from_str(&body)?;
            let pretty = serde_json::to_string_pretty(&value)?;
            fs::write(output, format!("{pretty}\n"))?;
        }
    }
    Ok(())
}
