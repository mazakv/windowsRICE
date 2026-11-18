use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Version of the on-disk entry format.
pub const ENTRY_FORMAT: u32 = 1;

/// Identifier shared by all entries that belong to one change.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChangeId(pub String);

impl ChangeId {
    /// Generates an identifier that is unique per process and time.
    pub fn generate() -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        Self(format!(
            "{:x}-{:x}-{seq:x}",
            now_unix_ms(),
            std::process::id()
        ))
    }
}

impl std::fmt::Display for ChangeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Lifecycle phase of a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Recorded before anything is modified; carries the original value.
    Intent,
    /// The modification was performed.
    Applied,
    /// The new state was read back and matches the expected value.
    Verified,
    /// Terminal: the change is complete.
    Committed,
    /// Terminal: the original state was restored.
    RolledBack,
    /// The operation failed. Not terminal: the state may be partially changed
    /// and must be inspected by recovery.
    Failed,
}

impl Phase {
    pub fn is_terminal(self) -> bool {
        matches!(self, Phase::Committed | Phase::RolledBack)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RegistryHive {
    Hkcu,
    Hklm,
}

/// What a change modifies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    Registry {
        hive: RegistryHive,
        key: String,
        value_name: String,
    },
    Wallpaper {
        monitor_id: String,
    },
    TaskbarAutoHide,
    StartupRegistration {
        mechanism: String,
    },
    File {
        path: PathBuf,
    },
}

/// A recorded value. `Absent` means the value did not exist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum RecordedValue {
    Absent,
    Bool(bool),
    Dword(u32),
    Qword(u64),
    String(String),
    ExpandString(String),
    MultiString(Vec<String>),
    /// Lowercase hexadecimal bytes.
    BinaryHex(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub format: u32,
    pub change_id: ChangeId,
    pub phase: Phase,
    pub timestamp_unix_ms: u64,
    pub target: Target,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<RecordedValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<RecordedValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

pub(crate) fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}
