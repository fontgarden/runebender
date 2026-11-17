// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Bounded retry receipts that evict the oldest finished entry.
//!
//! A ledger maps a retry identity, usually an actor and an operation key, to the receipt that
//! the first request produced.
//! A repeated request reads the receipt instead of executing again.
//! The ledger holds at most its capacity of entries.
//! When it is full, a new entry replaces the oldest entry that the caller reports as finished.
//! An entry that is still in progress is never evicted, so a retry of active work always replays.
//! A retry of an evicted entry is a new request.

use std::collections::BTreeMap;

/// The ledger is full and every retained entry is still in progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReceiptLedgerFull;

/// Bounded map from retry identity to receipt, in insertion order.
#[derive(Clone, Debug)]
pub struct ReceiptLedger<K, R> {
    capacity: usize,
    next_sequence: u64,
    entries: BTreeMap<K, (u64, R)>,
    order: BTreeMap<u64, K>,
}

impl<K: Ord + Clone, R> ReceiptLedger<K, R> {
    /// Create an empty ledger that holds at most `capacity` entries.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            next_sequence: 0,
            entries: BTreeMap::new(),
            order: BTreeMap::new(),
        }
    }

    /// Maximum number of retained entries.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Number of retained entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the ledger holds no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The receipt retained for `key`, if any.
    pub fn get(&self, key: &K) -> Option<&R> {
        self.entries.get(key).map(|(_, receipt)| receipt)
    }

    /// Retained receipts in insertion order, oldest first.
    pub fn values(&self) -> impl Iterator<Item = &R> {
        self.order
            .values()
            .filter_map(|key| self.entries.get(key).map(|(_, receipt)| receipt))
    }

    /// Make room for one new entry.
    ///
    /// When the ledger is full, the oldest entry for which `finished` returns true is removed.
    /// The ledger is unchanged and the call fails when every entry is still in progress.
    pub fn make_room(
        &mut self,
        mut finished: impl FnMut(&K, &R) -> bool,
    ) -> Result<(), ReceiptLedgerFull> {
        if self.entries.len() < self.capacity {
            return Ok(());
        }
        let oldest_finished = self.order.iter().find_map(|(sequence, key)| {
            let (_, receipt) = self.entries.get(key)?;
            finished(key, receipt).then_some(*sequence)
        });
        let Some(sequence) = oldest_finished else {
            return Err(ReceiptLedgerFull);
        };
        if let Some(key) = self.order.remove(&sequence) {
            self.entries.remove(&key);
        }
        Ok(())
    }

    /// Retain `receipt` for `key` as the newest entry and return the receipt it replaced.
    ///
    /// A full ledger evicts its oldest entry, so the capacity always holds.
    /// A ledger whose entries can be in progress calls [`Self::make_room`] first.
    pub fn insert(&mut self, key: K, receipt: R) -> Option<R> {
        let replaced = self.entries.remove(&key).map(|(sequence, receipt)| {
            self.order.remove(&sequence);
            receipt
        });
        if self.entries.len() >= self.capacity
            && let Some((_, oldest)) = self.order.pop_first()
        {
            self.entries.remove(&oldest);
        }
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        self.order.insert(sequence, key.clone());
        self.entries.insert(key, (sequence, receipt));
        replaced
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_ledger_evicts_the_oldest_finished_entry() {
        let mut ledger = ReceiptLedger::new(3);
        for (key, finished) in [(1, false), (2, true), (3, true)] {
            assert_eq!(ledger.make_room(|_, _| true), Ok(()));
            ledger.insert(key, finished);
        }
        assert_eq!(ledger.make_room(|_, finished| *finished), Ok(()));
        assert_eq!(ledger.get(&1), Some(&false));
        assert_eq!(ledger.get(&2), None);
        ledger.insert(4, false);
        assert_eq!(ledger.make_room(|_, finished| *finished), Ok(()));
        assert_eq!(ledger.get(&3), None);
        ledger.insert(5, false);
        assert_eq!(
            ledger.make_room(|_, finished| *finished),
            Err(ReceiptLedgerFull)
        );
        assert_eq!(ledger.len(), 3);
        assert_eq!(
            ledger.values().copied().collect::<Vec<_>>(),
            [false, false, false]
        );
    }

    #[test]
    fn reinserting_a_key_moves_it_to_the_newest_position() {
        let mut ledger = ReceiptLedger::new(2);
        ledger.insert("a", 1);
        ledger.insert("b", 2);
        assert_eq!(ledger.insert("a", 3), Some(1));
        assert_eq!(ledger.make_room(|_, _| true), Ok(()));
        assert_eq!(ledger.get(&"b"), None);
        assert_eq!(ledger.get(&"a"), Some(&3));
    }
}
