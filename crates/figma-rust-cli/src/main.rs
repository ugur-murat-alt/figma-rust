mod compiler;
mod server;
mod verify;

use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "figma-rust",
    version,
    about = "Deterministic Figma to GPUI compiler"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the pinned compiler identity.
    Version,
    /// Show the normalized diagnostics and raw node tree.
    Inspect {
        /// Raw extraction bundle JSON.
        raw: PathBuf,
        /// Emit a machine-readable report.
        #[arg(long)]
        json: bool,
    },
    /// Normalize an extraction bundle and report diagnostics.
    Lint {
        /// Raw extraction bundle JSON.
        raw: PathBuf,
        /// Emit a machine-readable report.
        #[arg(long)]
        json: bool,
        /// Treat warnings as a failed lint.
        #[arg(long)]
        strict: bool,
    },
    /// Normalize and generate deterministic GPUI Rust artifacts.
    Compile {
        /// Raw extraction bundle JSON.
        raw: PathBuf,
        /// Output directory. Only fixed artifact names are written.
        #[arg(long)]
        out: PathBuf,
    },
    /// Run the loopback-only compiler bridge used by the Figma plugin.
    Serve {
        /// Loopback TCP port.
        #[arg(long, default_value_t = 38_421)]
        port: u16,
        /// Persist exact POST /export bytes atomically to this fixed local path.
        #[arg(long, requires = "export_token_file")]
        export: Option<PathBuf>,
        /// File containing a short-lived export token (minimum 32 characters).
        #[arg(long, requires = "export")]
        export_token_file: Option<PathBuf>,
    },
    /// Compare node geometry and optional render images.
    Verify {
        /// Verification manifest JSON.
        manifest: PathBuf,
        /// Emit the machine-readable report.
        #[arg(long)]
        json: bool,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("figma-rust error: {error}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, compiler::CliError> {
    match cli.command {
        Command::Version => {
            println!(
                "figma-rust {} (GPUI {})",
                env!("CARGO_PKG_VERSION"),
                figma_rust_codegen::GPUI_REVISION
            );
            Ok(ExitCode::SUCCESS)
        }
        Command::Inspect { raw, json } => {
            let report = compiler::inspect_file(&raw)?;
            if json {
                print!("{}", compiler::pretty_json(&report)?);
            } else {
                print!("{}", report.text());
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Lint { raw, json, strict } => {
            let report = compiler::lint_file(&raw)?;
            if json {
                print!("{}", compiler::pretty_json(&report)?);
            } else {
                print!("{}", report.text());
            }
            Ok(if report.failed(strict) {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            })
        }
        Command::Compile { raw, out } => {
            let result = compiler::compile_file(&raw, &out)?;
            for diagnostic in &result.diagnostics {
                eprintln!("{}", compiler::format_diagnostic(diagnostic));
            }
            if let Some(error) = result.error {
                eprintln!("compile failed: {error}");
                Ok(ExitCode::from(1))
            } else {
                println!("wrote deterministic artifacts to {}", out.display());
                Ok(ExitCode::SUCCESS)
            }
        }
        Command::Serve {
            port,
            export,
            export_token_file,
        } => {
            server::serve(port, export.as_deref(), export_token_file.as_deref())?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Verify { manifest, json } => match verify::verify_manifest(&manifest) {
            Ok(report) => {
                if json {
                    print!("{}", compiler::pretty_json(&report)?);
                } else if report.passed {
                    println!("verification passed");
                } else {
                    for failure in &report.failures {
                        eprintln!("verification failed: {failure}");
                    }
                    for difference in &report.geometry_differences {
                        eprintln!(
                            "{} {}: expected {}, actual {}",
                            difference.node_id,
                            difference.property,
                            difference.expected,
                            difference.actual
                        );
                    }
                }

                Ok(if report.passed {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::from(1)
                })
            }
            Err(error) => {
                eprintln!("verification error: {error}");
                Ok(ExitCode::from(2))
            }
        },
    }
}
