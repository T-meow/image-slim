use clap::{Parser, Subcommand};
use image_slim_agent::AgentService;
use image_slim_agent::mcp;
use image_slim_agent::protocol::{CompressRequest, Envelope, PlanRequest};
use image_slim_core::error::{AppError, ErrorCode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(name = "image-slim-agent", version, about)]
struct Cli {
    #[arg(long, global = true, value_name = "ABSOLUTE_PATH")]
    allow_root: Vec<PathBuf>,
    #[arg(long, global = true)]
    allow_overwrite: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Capabilities {
        #[arg(long)]
        json: bool,
    },
    Plan {
        #[arg(long, value_name = "-")]
        request: String,
    },
    Compress {
        #[arg(long, value_name = "-")]
        request: String,
    },
    Mcp,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let service = match AgentService::new(cli.allow_root, cli.allow_overwrite) {
        Ok(service) => service,
        Err(error) => {
            eprintln!("image-slim-agent startup failed: {error}");
            return ExitCode::from(2);
        }
    };

    match cli.command {
        Command::Capabilities { json } => {
            if !json {
                return write_envelope(Envelope::<serde_json::Value>::failure(
                    AppError::new(ErrorCode::InvalidRequest).detail("capabilities requires --json"),
                ));
            }
            write_envelope(service.capabilities())
        }
        Command::Plan { request } => match read_request::<PlanRequest>(&request) {
            Ok(request) => write_envelope(service.plan_cli(request)),
            Err(error) => write_envelope(Envelope::<serde_json::Value>::failure(error)),
        },
        Command::Compress { request } => match read_request::<CompressRequest>(&request) {
            Ok(request) if request.plan_id.is_some() || request.paths.is_none() => {
                write_envelope(Envelope::<serde_json::Value>::failure(
                    AppError::new(ErrorCode::InvalidRequest)
                        .detail("CLI compress requires paths and does not accept plan_id"),
                ))
            }
            Ok(request) => {
                let worker_service = service.clone();
                let worker =
                    tokio::task::spawn_blocking(move || worker_service.compress_and_wait(request));
                tokio::pin!(worker);
                let response = tokio::select! {
                    response = &mut worker => response.unwrap_or_else(|error| {
                        Envelope::failure(AppError::internal(error))
                    }),
                    signal = tokio::signal::ctrl_c() => {
                        if let Err(error) = signal {
                            eprintln!("failed to install Ctrl+C handler: {error}");
                        }
                        service.cancel_active();
                        worker.await.unwrap_or_else(|error| {
                            Envelope::failure(AppError::internal(error))
                        })
                    }
                };
                write_envelope(response)
            }
            Err(error) => write_envelope(Envelope::<serde_json::Value>::failure(error)),
        },
        Command::Mcp => match mcp::serve(service).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("image-slim-agent MCP transport failed: {error:#}");
                ExitCode::from(1)
            }
        },
    }
}

fn read_request<T: DeserializeOwned>(source: &str) -> Result<T, AppError> {
    if source != "-" {
        return Err(AppError::new(ErrorCode::InvalidRequest)
            .detail("--request currently accepts only '-' for stdin"));
    }
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| AppError::io(error, "stdin"))?;
    serde_json::from_str(&input)
        .map_err(|error| AppError::new(ErrorCode::InvalidRequest).detail(error))
}

fn write_envelope<T: Serialize>(envelope: Envelope<T>) -> ExitCode {
    let success = envelope.is_success();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    if serde_json::to_writer(&mut output, &envelope).is_err() || writeln!(output).is_err() {
        return ExitCode::from(1);
    }
    if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
