//! Generates `openapi.json` from the compiled OpenAPI spec.
//!
//! Usage:
//!   cargo run --bin generate_openapi > openapi.json
//!   cargo run --bin generate_openapi -- --output openapi.json

use std::{
    env, fs,
    io::{self, Write},
    path::PathBuf,
};

use smart_home_service::api::handlers::ApiDoc;
use utoipa::OpenApi;

fn main() {
    let json = ApiDoc::openapi()
        .to_pretty_json()
        .expect("Failed to serialise OpenAPI spec");

    // Parse optional `--output <path>` argument.
    let args: Vec<String> = env::args().collect();
    let output_path: Option<PathBuf> = args
        .windows(2)
        .find(|w| w[0] == "--output")
        .map(|w| PathBuf::from(&w[1]));

    match output_path {
        Some(path) => {
            fs::write(&path, &json).unwrap_or_else(|e| {
                eprintln!("Error writing to {}: {e}", path.display());
                std::process::exit(1);
            });
            eprintln!("OpenAPI spec written to {}", path.display());
        }
        None => {
            io::stdout()
                .write_all(json.as_bytes())
                .expect("Failed to write to stdout");
        }
    }
}
