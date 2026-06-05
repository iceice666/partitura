mod output;

use clap::{Args, Parser, Subcommand};
use echo::{
    Context, Error, Event, Options, Provider, complete, get_model, get_models, get_providers,
};
use futures::StreamExt;
use std::io::{self, Read};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "echo", version, about = "Model provider client")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Run(RunArgs),
    Repl(ReplArgs),
    Login(ProviderArgs),
    Logout(ProviderArgs),
    Providers,
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Debug, Args)]
struct RunArgs {
    #[arg(long)]
    model: Option<String>,
    #[arg(long, alias = "complete")]
    json: bool,
}

#[derive(Debug, Args)]
struct ReplArgs {
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    system: Option<String>,
}

#[derive(Debug, Args)]
struct ProviderArgs {
    provider: Provider,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    Show,
}

#[tokio::main]
async fn main() {
    init_tracing();
    echo::register_default_adapters();

    if let Err(err) = run(Cli::parse()).await {
        tracing::error!("{err}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), Error> {
    match cli.command {
        Command::Run(args) => run_once(args).await,
        Command::Repl(args) => run_repl(args).await,
        Command::Login(args) => {
            echo::login(args.provider).await?;
            Ok(())
        }
        Command::Logout(args) => {
            echo::logout(args.provider).await?;
            Ok(())
        }
        Command::Providers => {
            output::write_json(&provider_report())?;
            Ok(())
        }
        Command::Config {
            command: ConfigCommand::Show,
        } => {
            output::write_json(&echo::resolved_config_view()?)?;
            Ok(())
        }
    }
}

async fn run_once(args: RunArgs) -> Result<(), Error> {
    let target = resolve_model_arg(args.model)?;
    let model = parse_model(&target)?;
    let ctx = read_context()?;
    let opts = Options::default();

    if args.json {
        let assistant = complete(&model, &ctx, &opts).await?;
        output::write_json(&assistant)?;
        return Ok(());
    }

    let mut stream = echo::stream(&model, &ctx, &opts);
    while let Some(event) = stream.next().await {
        output::write_event(&event)?;
    }
    let _ = stream.result().await?;
    Ok(())
}

async fn run_repl(args: ReplArgs) -> Result<(), Error> {
    let target = resolve_model_arg(args.model)?;
    let model = parse_model(&target)?;
    let config = echo::load_config().unwrap_or_default();
    let prompt = format!("{}> ", config.repl.prompt_prefix);
    let reply_prefix = format!("{}> ", config.repl.reply_prefix);
    let mut editor = rustyline::DefaultEditor::new().map_err(|err| Error::Cli(err.to_string()))?;
    let mut messages = Vec::new();

    loop {
        let line = match editor.readline(&prompt) {
            Ok(line) => line,
            Err(rustyline::error::ReadlineError::Interrupted) => continue,
            Err(rustyline::error::ReadlineError::Eof) => break,
            Err(err) => return Err(Error::Cli(err.to_string())),
        };

        messages.push(echo::Message::User {
            content: vec![echo::Block::Text {
                text: line,
                signature: None,
            }],
        });
        let ctx = Context {
            system_prompt: args.system.clone(),
            messages: messages.clone(),
            tools: Vec::new(),
        };
        let opts = Options::default();
        let abort = opts.abort.clone();
        let mut stream = echo::stream(&model, &ctx, &opts);

        output::write_text(&reply_prefix)?;
        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    abort.abort();
                }
                event = stream.next() => {
                    let Some(event) = event else {
                        break;
                    };
                    if let Event::TextDelta(delta) = event {
                        output::write_text(&delta.delta)?;
                    }
                }
            }
        }
        output::write_newline()?;
        match stream.result().await {
            Ok(assistant) => messages.push(echo::Message::Assistant(assistant)),
            Err(Error::Aborted) => continue,
            Err(err) => return Err(err),
        }
    }

    Ok(())
}

fn read_context() -> Result<Context, Error> {
    let mut body = String::new();
    io::stdin()
        .read_to_string(&mut body)
        .map_err(|err| Error::Cli(err.to_string()))?;
    serde_json::from_str(&body).map_err(Error::from)
}

fn resolve_model_arg(model: Option<String>) -> Result<String, Error> {
    if let Some(model) = model {
        return Ok(model);
    }
    if let Ok(model) = std::env::var("ECHO_MODEL") {
        return Ok(model);
    }
    let config = echo::load_config()?;
    config
        .default_model
        .ok_or_else(|| Error::Cli("missing --model, ECHO_MODEL, and default_model".to_string()))
}

fn parse_model(target: &str) -> Result<echo::Model, Error> {
    let (provider, id) = target
        .split_once('/')
        .ok_or_else(|| Error::Cli("model must be formatted as <provider>/<id>".to_string()))?;
    let provider: Provider = provider.parse()?;
    get_model(provider, id).ok_or_else(|| Error::UnknownModel(target.to_string()))
}

fn provider_report() -> Vec<serde_json::Value> {
    get_providers()
        .into_iter()
        .map(|provider| {
            serde_json::json!({
                "provider": provider,
                "models": get_models(provider),
                "credentials": echo::credential_status(provider),
            })
        })
        .collect()
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(io::stderr)
        .init();
}
