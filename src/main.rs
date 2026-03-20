use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use clap::{Parser, Subcommand};
use tracing::{error, info};

use inspec_rspec::error::{Error, Result};
use inspec_rspec::output;
use inspec_rspec::parser;
use inspec_rspec::transpiler;

#[derive(Parser)]
#[command(
    name = "inspec-rspec",
    version,
    about = "Deterministic InSpec control to RSpec compliance test transpiler"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Transpile InSpec controls to RSpec compliance tests.
    Transpile {
        /// Path to InSpec profile directory (containing controls/).
        profile_dir: String,

        /// Output directory for generated RSpec tests.
        #[arg(short, long, default_value = "generated/spec")]
        output: String,
    },

    /// Parse and display InSpec controls without generating output.
    Inspect {
        /// Path to InSpec profile directory.
        profile_dir: String,
    },

    /// Hash the generated RSpec output for attestation.
    Hash {
        /// Path to generated spec directory.
        spec_dir: String,
    },
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    let result = match cli.command {
        Command::Transpile {
            profile_dir,
            output,
        } => run_transpile(&profile_dir, &output),
        Command::Inspect { profile_dir } => run_inspect(&profile_dir),
        Command::Hash { spec_dir } => run_hash(&spec_dir),
    };

    if let Err(e) = result {
        error!("{e}");
        process::exit(1);
    }
}

/// Discover and read all InSpec control files in a profile directory.
fn discover_controls(profile_dir: &str) -> Result<Vec<(String, String)>> {
    let profile_path = PathBuf::from(profile_dir);

    // Look for controls/ subdirectory
    let controls_dir = profile_path.join("controls");
    let search_dir = if controls_dir.is_dir() {
        controls_dir
    } else if profile_path.is_dir() {
        profile_path.clone()
    } else {
        return Err(Error::InvalidProfile {
            path: profile_path,
        });
    };

    let mut files: Vec<(String, String)> = Vec::new();

    let entries = fs::read_dir(&search_dir).map_err(|e| Error::Io {
        path: search_dir.clone(),
        source: e,
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| Error::Io {
            path: search_dir.clone(),
            source: e,
        })?;

        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "rb") {
            let content = fs::read_to_string(&path).map_err(|e| Error::Io {
                path: path.clone(),
                source: e,
            })?;
            let filename = path
                .file_name()
                .map_or_else(|| "unknown.rb".to_string(), |n| n.to_string_lossy().to_string());
            files.push((filename, content));
        }
    }

    // Sort for deterministic output
    files.sort_by(|a, b| a.0.cmp(&b.0));

    Ok(files)
}

/// Extract profile name from directory path.
fn profile_name(profile_dir: &str) -> String {
    Path::new(profile_dir)
        .file_name()
        .map_or_else(|| "unknown".to_string(), |n| n.to_string_lossy().to_string())
}

/// Run the transpile command.
fn run_transpile(profile_dir: &str, output_dir: &str) -> Result<()> {
    info!("Transpiling InSpec profile: {profile_dir}");

    let files = discover_controls(profile_dir)?;
    if files.is_empty() {
        return Err(Error::NoControls {
            path: PathBuf::from(profile_dir),
        });
    }

    let mut all_controls = Vec::new();
    for (filename, content) in &files {
        let controls = parser::parse_controls(content, filename)?;
        info!("Parsed {} controls from {filename}", controls.len());
        all_controls.extend(controls);
    }

    if all_controls.is_empty() {
        return Err(Error::NoControls {
            path: PathBuf::from(profile_dir),
        });
    }

    let name = profile_name(profile_dir);
    let rspec_files = transpiler::transpile_profile(&all_controls, &name);

    let output_path = Path::new(output_dir);
    let written = output::write_rspec_files(&rspec_files, output_path)?;
    let helpers_path = output::write_helpers(output_path)?;
    let spec_helper_path = output::write_spec_helper(output_path, &name)?;

    info!("Generated {} RSpec test files", written.len());
    info!("Helpers: {helpers_path}");
    info!("Spec helper: {spec_helper_path}");

    for path in &written {
        info!("  {path}");
    }

    Ok(())
}

/// Run the inspect command — parse and display controls.
fn run_inspect(profile_dir: &str) -> Result<()> {
    info!("Inspecting InSpec profile: {profile_dir}");

    let files = discover_controls(profile_dir)?;
    if files.is_empty() {
        return Err(Error::NoControls {
            path: PathBuf::from(profile_dir),
        });
    }

    let mut all_controls = Vec::new();
    for (filename, content) in &files {
        let controls = parser::parse_controls(content, filename)?;
        all_controls.extend(controls);
    }

    let json = serde_json::to_string_pretty(&all_controls).map_err(|e| Error::Transpile {
        control_id: String::new(),
        message: format!("JSON serialization failed: {e}"),
    })?;

    println!("{json}");

    Ok(())
}

/// Run the hash command — compute BLAKE3 hash of generated output.
fn run_hash(spec_dir: &str) -> Result<()> {
    info!("Hashing spec directory: {spec_dir}");

    let spec_path = PathBuf::from(spec_dir);
    if !spec_path.is_dir() {
        return Err(Error::InvalidProfile { path: spec_path });
    }

    // Collect all .rb files, sorted for determinism
    let mut file_contents: Vec<(String, Vec<u8>)> = Vec::new();

    fn collect_rb_files(dir: &Path, files: &mut Vec<(String, Vec<u8>)>) -> Result<()> {
        let entries = fs::read_dir(dir).map_err(|e| Error::Io {
            path: dir.to_path_buf(),
            source: e,
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| Error::Io {
                path: dir.to_path_buf(),
                source: e,
            })?;

            let path = entry.path();
            if path.is_dir() {
                collect_rb_files(&path, files)?;
            } else if path.extension().is_some_and(|ext| ext == "rb") {
                let content = fs::read(&path).map_err(|e| Error::Io {
                    path: path.clone(),
                    source: e,
                })?;
                let rel = path.display().to_string();
                files.push((rel, content));
            }
        }
        Ok(())
    }

    collect_rb_files(&spec_path, &mut file_contents)?;
    file_contents.sort_by(|a, b| a.0.cmp(&b.0));

    // Simple deterministic hash: concatenate all file contents with separators
    // In production, this would use BLAKE3 from tameshi. For now, use a simple
    // checksum approach that can be replaced.
    let mut hasher_input = Vec::new();
    for (name, content) in &file_contents {
        hasher_input.extend_from_slice(name.as_bytes());
        hasher_input.push(0);
        hasher_input.extend_from_slice(content);
        hasher_input.push(0);
    }

    // Simple hash for now — will be replaced with BLAKE3 when tameshi is integrated
    let hash = simple_hash(&hasher_input);
    println!("{hash}");

    Ok(())
}

/// Simple deterministic hash (placeholder for BLAKE3).
fn simple_hash(data: &[u8]) -> String {
    // FNV-1a 128-bit hash — deterministic, fast, good distribution
    // Will be replaced with BLAKE3 when tameshi dependency is added
    let mut h: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    let prime: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    for &byte in data {
        h ^= u128::from(byte);
        h = h.wrapping_mul(prime);
    }
    format!("{h:032x}")
}
