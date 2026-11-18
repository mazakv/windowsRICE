use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::entry::{
    ChangeId, ENTRY_FORMAT, JournalEntry, Phase, RecordedValue, Target, now_unix_ms,
};

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("journal I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to serialize journal entry: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("phase {0:?} cannot be recorded here; use Journal::begin for intents")]
    InvalidPhase(Phase),
}

/// Handle to an append-only JSONL journal file.
#[derive(Debug, Clone)]
pub struct Journal {
    path: PathBuf,
}

/// A change that has been started with [`Journal::begin`].
#[derive(Debug, Clone)]
pub struct ChangeHandle {
    pub id: ChangeId,
    pub target: Target,
    pub description: String,
}

/// A line that could not be parsed. A truncated final line is the expected
/// result of a crash during a write; it is reported, never silently dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorruptLine {
    pub line_number: usize,
    pub error: String,
}

#[derive(Debug, Default)]
pub struct JournalReadout {
    pub entries: Vec<JournalEntry>,
    pub corrupt_lines: Vec<CorruptLine>,
}

/// The latest known state of one change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeSummary {
    pub id: ChangeId,
    pub target: Target,
    pub description: String,
    pub last_phase: Phase,
    /// Original value from the intent entry, if one was recorded.
    pub original: Option<RecordedValue>,
    pub last_timestamp_unix_ms: u64,
}

impl Journal {
    /// Creates a handle. Does not touch the file system.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Appends one entry and flushes it to disk before returning.
    pub fn append(&self, entry: &JournalEntry) -> Result<(), JournalError> {
        let io = |source| JournalError::Io {
            path: self.path.clone(),
            source,
        };
        let mut line = serde_json::to_string(entry)?;
        line.push('\n');
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(io)?;
        file.write_all(line.as_bytes()).map_err(io)?;
        file.sync_data().map_err(io)
    }

    /// Records the intent to change `target`, including its original value.
    /// Must be called before the change is made.
    pub fn begin(
        &self,
        target: Target,
        original: RecordedValue,
        description: impl Into<String>,
    ) -> Result<ChangeHandle, JournalError> {
        let handle = ChangeHandle {
            id: ChangeId::generate(),
            target,
            description: description.into(),
        };
        self.append(&JournalEntry {
            format: ENTRY_FORMAT,
            change_id: handle.id.clone(),
            phase: Phase::Intent,
            timestamp_unix_ms: now_unix_ms(),
            target: handle.target.clone(),
            description: handle.description.clone(),
            before: Some(original),
            after: None,
            detail: None,
        })?;
        Ok(handle)
    }

    /// Records a later phase of a change started with [`Journal::begin`].
    pub fn record(
        &self,
        handle: &ChangeHandle,
        phase: Phase,
        after: Option<RecordedValue>,
        detail: Option<String>,
    ) -> Result<(), JournalError> {
        if phase == Phase::Intent {
            return Err(JournalError::InvalidPhase(phase));
        }
        self.append(&JournalEntry {
            format: ENTRY_FORMAT,
            change_id: handle.id.clone(),
            phase,
            timestamp_unix_ms: now_unix_ms(),
            target: handle.target.clone(),
            description: handle.description.clone(),
            before: None,
            after,
            detail,
        })
    }

    /// Reads all entries. A missing file is an empty journal. Lines that
    /// cannot be parsed are reported in [`JournalReadout::corrupt_lines`].
    pub fn read(&self) -> Result<JournalReadout, JournalError> {
        let bytes = match std::fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(JournalReadout::default());
            }
            Err(source) => {
                return Err(JournalError::Io {
                    path: self.path.clone(),
                    source,
                });
            }
        };
        let mut out = JournalReadout::default();
        for (index, raw) in bytes.split(|b| *b == b'\n').enumerate() {
            let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
            if raw.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            match serde_json::from_slice::<JournalEntry>(raw) {
                Ok(entry) => out.entries.push(entry),
                Err(e) => out.corrupt_lines.push(CorruptLine {
                    line_number: index + 1,
                    error: e.to_string(),
                }),
            }
        }
        Ok(out)
    }
}

impl JournalReadout {
    /// Latest state of every change, in order of first appearance.
    pub fn changes(&self) -> Vec<ChangeSummary> {
        let mut out: Vec<ChangeSummary> = Vec::new();
        for e in &self.entries {
            match out.iter_mut().find(|c| c.id == e.change_id) {
                Some(c) => {
                    c.last_phase = e.phase;
                    c.last_timestamp_unix_ms = e.timestamp_unix_ms;
                    if e.phase == Phase::Intent && c.original.is_none() {
                        c.original.clone_from(&e.before);
                    }
                }
                None => out.push(ChangeSummary {
                    id: e.change_id.clone(),
                    target: e.target.clone(),
                    description: e.description.clone(),
                    last_phase: e.phase,
                    original: if e.phase == Phase::Intent {
                        e.before.clone()
                    } else {
                        None
                    },
                    last_timestamp_unix_ms: e.timestamp_unix_ms,
                }),
            }
        }
        out
    }

    /// Changes that never reached `Committed` or `RolledBack`. These are what
    /// the recovery utility must present to the user.
    pub fn unresolved(&self) -> Vec<ChangeSummary> {
        self.changes()
            .into_iter()
            .filter(|c| !c.last_phase.is_terminal())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::RegistryHive;

    fn reg_target() -> Target {
        Target::Registry {
            hive: RegistryHive::Hkcu,
            key: r"Software\windowsRICE\Test".into(),
            value_name: "Value".into(),
        }
    }

    fn journal() -> (tempfile::TempDir, Journal) {
        let dir = tempfile::tempdir().unwrap();
        let j = Journal::new(dir.path().join("journal").join("changes.jsonl"));
        (dir, j)
    }

    #[test]
    fn missing_file_is_empty_journal() {
        let (_d, j) = journal();
        let r = j.read().unwrap();
        assert!(r.entries.is_empty() && r.corrupt_lines.is_empty());
        assert!(!j.path().exists());
    }

    #[test]
    fn full_lifecycle_is_resolved() {
        let (_d, j) = journal();
        let h = j
            .begin(reg_target(), RecordedValue::Absent, "test change")
            .unwrap();
        j.record(&h, Phase::Applied, Some(RecordedValue::Dword(1)), None)
            .unwrap();
        j.record(&h, Phase::Verified, Some(RecordedValue::Dword(1)), None)
            .unwrap();
        j.record(&h, Phase::Committed, None, None).unwrap();

        let r = j.read().unwrap();
        assert_eq!(r.entries.len(), 4);
        let changes = r.changes();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].last_phase, Phase::Committed);
        assert_eq!(changes[0].original, Some(RecordedValue::Absent));
        assert!(r.unresolved().is_empty());
    }

    #[test]
    fn crash_after_apply_is_unresolved_with_original_value() {
        let (_d, j) = journal();
        let done = j
            .begin(reg_target(), RecordedValue::Dword(0), "done")
            .unwrap();
        j.record(
            &done,
            Phase::RolledBack,
            Some(RecordedValue::Dword(0)),
            None,
        )
        .unwrap();
        let crashed = j
            .begin(
                reg_target(),
                RecordedValue::String("원래 값".into()),
                "crashed",
            )
            .unwrap();
        j.record(
            &crashed,
            Phase::Applied,
            Some(RecordedValue::String("new".into())),
            None,
        )
        .unwrap();

        let unresolved = j.read().unwrap().unresolved();
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].id, crashed.id);
        assert_eq!(unresolved[0].last_phase, Phase::Applied);
        assert_eq!(
            unresolved[0].original,
            Some(RecordedValue::String("원래 값".into()))
        );
    }

    #[test]
    fn failed_changes_remain_unresolved() {
        let (_d, j) = journal();
        let h = j
            .begin(reg_target(), RecordedValue::Dword(3), "fails")
            .unwrap();
        j.record(&h, Phase::Failed, None, Some("access denied".into()))
            .unwrap();
        assert_eq!(j.read().unwrap().unresolved().len(), 1);
    }

    #[test]
    fn truncated_tail_is_reported_and_earlier_entries_survive() {
        let (_d, j) = journal();
        let h = j.begin(reg_target(), RecordedValue::Absent, "a").unwrap();
        j.record(&h, Phase::Applied, Some(RecordedValue::Dword(1)), None)
            .unwrap();
        let mut f = OpenOptions::new().append(true).open(j.path()).unwrap();
        f.write_all(br#"{"format":1,"change_id":"x","pha"#).unwrap();
        drop(f);

        let r = j.read().unwrap();
        assert_eq!(r.entries.len(), 2);
        assert_eq!(r.corrupt_lines.len(), 1);
        assert_eq!(r.corrupt_lines[0].line_number, 3);
        assert_eq!(r.unresolved().len(), 1);
    }

    #[test]
    fn record_rejects_intent_phase() {
        let (_d, j) = journal();
        let h = j
            .begin(Target::TaskbarAutoHide, RecordedValue::Bool(false), "x")
            .unwrap();
        assert!(matches!(
            j.record(&h, Phase::Intent, None, None),
            Err(JournalError::InvalidPhase(Phase::Intent))
        ));
    }

    #[test]
    fn each_entry_is_a_single_line() {
        let (_d, j) = journal();
        j.begin(
            Target::File {
                path: PathBuf::from("C:\\a\nb"),
            },
            RecordedValue::MultiString(vec!["line1\nline2".into()]),
            "multi\nline",
        )
        .unwrap();
        let text = std::fs::read_to_string(j.path()).unwrap();
        assert_eq!(text.lines().count(), 1);
        assert_eq!(j.read().unwrap().entries.len(), 1);
    }

    #[test]
    fn change_ids_are_unique() {
        let a = ChangeId::generate();
        let b = ChangeId::generate();
        assert_ne!(a, b);
    }
}
