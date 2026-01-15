//! Build-time code generator for SnugomClient.
//!
//! This crate scans your source files for `#[derive(SnugomEntity)]` structs
//! and generates a `SnugomClient` with typed accessor methods.
//!
//! # Example
//!
//! In your `build.rs`:
//!
//! ```ignore
//! fn main() {
//!     snugom_build::generate_client()
//!         .scan_path("src/")
//!         .output_file("src/generated/snugom_client.rs")
//!         .run()
//!         .expect("Failed to generate SnugomClient");
//!
//!     println!("cargo:rerun-if-changed=src/");
//! }
//! ```

mod generator;
mod scanner;

pub use generator::ClientGenerator;

/// Create a new client generator with default settings.
///
/// # Example
///
/// ```ignore
/// snugom_build::generate_client()
///     .scan_path("src/")
///     .output_file("src/generated/snugom_client.rs")
///     .run()
///     .expect("Failed to generate SnugomClient");
/// ```
pub fn generate_client() -> ClientGenerator {
    ClientGenerator::new()
}
