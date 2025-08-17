use crate::cli::commands::{GroupAction, OutputFormat};
use anyhow::Result;
use std::path::Path;

// Temporarily disabled during refactoring to device types
pub async fn group(
    _action: GroupAction,
    _format: OutputFormat,
    color: bool,
    _cache_dir: Option<&Path>,
) -> Result<()> {
    use crate::output::print_info;
    print_info(
        "Group management commands are being refactored to use automatic device types",
        color,
    );
    print_info(
        "Use --device-type parameter with monitor and bulk commands instead",
        color,
    );
    Ok(())
}
