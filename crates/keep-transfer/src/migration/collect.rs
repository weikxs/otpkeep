//! Collecting the codes of a multi-code export as they are scanned (PRD FR-7.2, design/05-transfer.md §5.3).

use std::collections::BTreeMap;

use super::{MigrationBatch, MigrationItem};

/// What happened to a scanned code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Added {
    /// A code not seen before.
    New,
    /// The same code was already scanned; nothing changed.
    AlreadyHad,
    /// The code's position is outside its export's size; it was not kept.
    OutOfRange,
}

/// Progress of one export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchStatus {
    /// Shared by all codes of the export.
    pub batch_id: i32,
    /// Positions received, ascending.
    pub received: Vec<u32>,
    /// How many codes the export has.
    pub expected: u32,
    /// Every code has been received.
    pub complete: bool,
    /// Codes of this export disagree about its size; the first value is used.
    pub size_conflict: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Group {
    batch_id: i32,
    expected: u32,
    size_conflict: bool,
    codes: BTreeMap<u32, MigrationBatch>,
}

/// The codes scanned so far, grouped by export, in the order the exports were first seen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationCollector {
    groups: Vec<Group>,
}

impl MigrationCollector {
    /// Adds one scanned code.
    #[must_use]
    pub fn with(&self, batch: MigrationBatch) -> (Self, Added) {
        let mut groups = self.groups.clone();
        let index = if let Some(index) = groups.iter().position(|g| g.batch_id == batch.batch_id) {
            index
        } else {
            groups.push(Group {
                batch_id: batch.batch_id,
                // A code without a size is read as a single-code export.
                expected: batch.batch_size.max(1),
                size_conflict: false,
                codes: BTreeMap::new(),
            });
            groups.len() - 1
        };
        let group = &mut groups[index];
        if batch.batch_index >= group.expected {
            if group.codes.is_empty() {
                groups.remove(index);
            }
            return (Self { groups }, Added::OutOfRange);
        }
        if group.codes.contains_key(&batch.batch_index) {
            return (self.clone(), Added::AlreadyHad);
        }
        if batch.batch_size.max(1) != group.expected {
            group.size_conflict = true;
        }
        group.codes.insert(batch.batch_index, batch);
        (Self { groups }, Added::New)
    }

    /// Progress of every export seen.
    #[must_use]
    pub fn groups(&self) -> Vec<BatchStatus> {
        self.groups
            .iter()
            .map(|g| {
                let received: Vec<u32> = g.codes.keys().copied().collect();
                BatchStatus {
                    batch_id: g.batch_id,
                    complete: received.len() == usize::try_from(g.expected).unwrap_or(usize::MAX),
                    received,
                    expected: g.expected,
                    size_conflict: g.size_conflict,
                }
            })
            .collect()
    }

    /// The accounts of one export received so far, in code order; `None` for an unknown export.
    #[must_use]
    pub fn take(&self, batch_id: i32) -> Option<Vec<MigrationItem>> {
        let group = self.groups.iter().find(|g| g.batch_id == batch_id)?;
        Some(
            group
                .codes
                .values()
                .flat_map(|code| code.items.iter().cloned())
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests;
