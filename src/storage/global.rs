use std::sync::LazyLock;
use crate::storage::MemoryStorage;

/// Global storage instance for the CLI application
pub static GLOBAL_STORAGE: LazyLock<MemoryStorage> = LazyLock::new(MemoryStorage::new);