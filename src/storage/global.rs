use crate::storage::MemoryStorage;
use std::sync::LazyLock;

/// Global storage instance for the CLI application
pub static GLOBAL_STORAGE: LazyLock<MemoryStorage> = LazyLock::new(MemoryStorage::new);
