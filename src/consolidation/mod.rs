//! Long-Term Memory Consolidation Types & Store
//!
//! Provides temporal memory records (`TemporalRecord`, `CompactedContent`, `CollectedItem`,
//! `SourceType`, `RecordImportance`) and persistent storage (`LongTermStore`).

pub mod store;
pub mod temporal;

pub use store::{LongTermStore, StoreResult};
pub use temporal::{
    CollectedItem, CompactedContent, ConsolidationReport, RecordImportance, SourceType,
    TemporalRecord,
};
