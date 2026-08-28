mod compiler;
mod generation;
mod server;
mod verify;

use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum DiagnosticDetail {
    /// Preserve the complete lossless diagnostic list.
    #[default]
    Full,
    /// Show deterministic groups with counts and at most three unique samples.
    Grouped,
}

impl DiagnosticDetail {
    const fn grouped(self) -> bool {
        matches!(self, Self::Grouped)
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CapabilityProfileArg {
    /// Minimal usage-led profile for the bounded OrbitLine/Core Controls path.
    OrbitlineMinimalV1,
}

impl CapabilityProfileArg {
    const fn id(self) -> &'static str {
        match self {
            Self::OrbitlineMinimalV1 => figma_rust_core::ORBITLINE_MINIMAL_PROFILE_V1,
        }
    }
}

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
        /// Human diagnostic detail; JSON always includes groups and the full list.
        #[arg(long, value_enum, default_value_t)]
        diagnostics: DiagnosticDetail,
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
        /// Human diagnostic detail; JSON always includes groups and the full list.
        #[arg(long, value_enum, default_value_t)]
        diagnostics: DiagnosticDetail,
    },
    /// Inventory advertised capabilities against a versioned local profile.
    Profile {
        /// Raw extraction bundle JSON.
        raw: PathBuf,
        /// Explicit versioned capability profile.
        #[arg(long, value_enum)]
        profile: CapabilityProfileArg,
        /// Emit the machine-readable report.
        #[arg(long)]
        json: bool,
    },
    /// Normalize and generate deterministic GPUI Rust artifacts.
    Compile {
        /// Raw extraction bundle JSON.
        raw: PathBuf,
        /// Output directory. Only fixed artifact names are written.
        #[arg(long)]
        out: PathBuf,
        /// Compile each selected root independently and publish root-status.json.
        #[arg(long)]
        root_scoped: bool,
        /// Fail before normalization when the bundle violates this capability profile.
        #[arg(long, value_enum, conflicts_with = "root_scoped")]
        profile: Option<CapabilityProfileArg>,
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
        Command::Inspect {
            raw,
            json,
            diagnostics,
        } => {
            let report = compiler::inspect_file(&raw)?;
            if json {
                print!("{}", compiler::pretty_json(&report)?);
            } else {
                print!("{}", report.text(diagnostics.grouped()));
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Lint {
            raw,
            json,
            strict,
            diagnostics,
        } => {
            let report = compiler::lint_file(&raw)?;
            if json {
                print!("{}", compiler::pretty_json(&report)?);
            } else {
                print!("{}", report.text(diagnostics.grouped()));
            }
            Ok(if report.failed(strict) {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            })
        }
        Command::Profile { raw, profile, json } => {
            let report = compiler::profile_file(&raw, profile.id())?;
            if json {
                print!("{}", compiler::pretty_json(&report)?);
            } else {
                println!(
                    "profile {}: {}; {} used, {} unused, {} violation(s)",
                    report.profile_id,
                    if report.passed { "passed" } else { "failed" },
                    report.summary.used,
                    report.summary.unused,
                    report.summary.violations
                );
                for diagnostic in &report.diagnostics {
                    eprintln!("{}", compiler::format_diagnostic(diagnostic));
                }
            }
            Ok(if report.passed {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            })
        }
        Command::Compile {
            raw,
            out,
            root_scoped,
            profile,
        } => run_compile(&raw, &out, root_scoped, profile),
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
                    print_verification_failures(&report);
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

fn run_compile(
    raw: &Path,
    out: &Path,
    root_scoped: bool,
    profile: Option<CapabilityProfileArg>,
) -> Result<ExitCode, compiler::CliError> {
    if root_scoped {
        let report = compiler::compile_file_root_scoped(raw, out)?;
        for outcome in &report.roots {
            for diagnostic in &outcome.diagnostics {
                eprintln!(
                    "root {}: {}",
                    outcome.root_id,
                    compiler::format_diagnostic(diagnostic)
                );
            }
        }
        println!(
            "wrote root-scoped status for {} root(s) to {}",
            report.summary.total,
            out.display()
        );
        Ok(if report.failed() {
            ExitCode::from(1)
        } else {
            ExitCode::SUCCESS
        })
    } else {
        let result = if let Some(profile) = profile {
            compiler::compile_file_with_profile(raw, out, profile.id())?
        } else {
            compiler::compile_file(raw, out)?
        };
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
}

fn print_verification_failures(report: &verify::VerificationReport) {
    for failure in &report.failures {
        eprintln!("verification failed: {failure}");
    }
    for difference in &report.geometry_differences {
        eprintln!(
            "{} {}: expected {}, actual {}",
            difference.node_id, difference.property, difference.expected, difference.actual
        );
    }
    for difference in &report.derived_geometry_differences {
        eprintln!(
            "{} {} relative to {}: expected {}, actual {}",
            difference.node_id,
            difference.property,
            difference.related_node_id,
            difference.expected,
            difference.actual
        );
    }
    for difference in &report.node_pixel_differences {
        eprintln!(
            "{} pixel difference: {} changed pixels, mean error {:.5}, max error {:.5}",
            difference.node_id,
            difference.changed_pixel_count,
            difference.mean_absolute_error,
            difference.maximum_pixel_error
        );
    }
    for diagnostic in &report.analysis_diagnostics {
        eprintln!(
            "{} {} {}: {}",
            diagnostic.code, diagnostic.node_id, diagnostic.property, diagnostic.message
        );
    }
}
