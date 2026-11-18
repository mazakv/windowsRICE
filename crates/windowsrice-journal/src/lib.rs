//! Append-only change journal.
//!
//! Every system change made by windowsRICE is recorded here *before* it is
//! applied (with the original value), and again after it is applied,
//! verified, committed or rolled back. The recovery utility reads the journal
//! independently of the main desktop to find changes that never reached a
//! terminal state, for example because the process crashed mid-change.
//!
//! Format: one JSON object per line (JSONL), each line self-describing.
//! Every append is flushed to disk before the call returns.

#![forbid(unsafe_code)]

mod entry;
mod store;

pub use entry::{ChangeId, ENTRY_FORMAT, JournalEntry, Phase, RecordedValue, RegistryHive, Target};
pub use store::{ChangeHandle, ChangeSummary, CorruptLine, Journal, JournalError, JournalReadout};
