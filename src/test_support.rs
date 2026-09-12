//! Test-support helpers compiled only for library unit tests (`#[cfg(test)]`).
//!
//! These paths live inside the library (not in `tests/`) so the *library* unit
//! tests do not depend on the *integration* test crate. Output paths are
//! relative to the package root, which `cargo test` uses as the working
//! directory on all platforms (macOS / Linux / Windows).

/// Returns a standardized test output path under `tests/output/{category}/`,
/// creating the directory if needed.
///
/// # Arguments
/// * `category` - The subdirectory name (e.g., "encode", "mux", "pcm")
/// * `filename` - The output filename
pub fn test_output_path(category: &str, filename: &str) -> std::path::PathBuf {
    let output_dir = std::path::PathBuf::from("tests/output").join(category);
    std::fs::create_dir_all(&output_dir).ok();
    output_dir.join(filename)
}

/// Removes a test output file if it exists.
/// Most unit-test modules use this, but the API is kept generally usable.
pub fn remove_test_output(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
}
