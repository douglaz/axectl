/// Integration tests for axectl
///
/// Run with: cargo test --test integration_tests
mod fixtures;
mod integration;

// Re-export test modules
pub use integration::*;
