use omnipaxos::storage::Storage;
use omnipaxos_storage::persistent_storage::{PersistentStorage, PersistentStorageConfig};
use tokio::sync::oneshot;

use crate::Entry;

pub struct MetaDataLog {
    pub idx: usize,
    pub entries: Vec<u32>,
    pub callback: oneshot::Sender<bool>,
}

pub struct ConsistentLogStore {
    inner: PersistentStorage<Entry>,
    commit_log_tx: oneshot::Sender<MetaDataLog>,
    // index where next append should go
    append_idx: usize,
}

impl ConsistentLogStore {
    pub fn new(config: PersistentStorageConfig, tx: oneshot::Sender<MetaDataLog>) -> Self {
        let inner = PersistentStorage::new(config);
        let append_idx = inner.get_log_len().unwrap();
        Self {
            inner,
            commit_log_tx: tx,
            append_idx,
        }
    }
}

impl Storage<Entry> for ConsistentLogStore {
    fn write_atomically(
        &mut self,
        ops: Vec<omnipaxos::storage::StorageOp<Entry>>,
    ) -> omnipaxos::storage::StorageResult<()> {
        let append_idx = ops.iter().fold(self.append_idx, |append_idx, op| match op {
            omnipaxos::storage::StorageOp::AppendEntry(_) => append_idx + 1,
            omnipaxos::storage::StorageOp::AppendEntries(entries) => append_idx + entries.len(),
            omnipaxos::storage::StorageOp::AppendOnPrefix(from_idx, entries) => from_idx + entries.len(),
            _ => append_idx,
        });

        self.inner.write_atomically(ops).inspect(|_| {
            self.append_idx = append_idx;
        })
    }

    fn append_entry(&mut self, entry: Entry) -> omnipaxos::storage::StorageResult<()> {
        self.inner.append_entry(entry).inspect(|_| {
            self.append_idx += 1;
        })
    }

    fn append_entries(&mut self, entries: Vec<Entry>) -> omnipaxos::storage::StorageResult<()> {
        let len = entries.len();
        self.inner.append_entries(entries).inspect(|_| {
            self.append_idx += len;
        })
    }

    fn append_on_prefix(&mut self, from_idx: usize, entries: Vec<Entry>) -> omnipaxos::storage::StorageResult<()> {
        let len = entries.len();
        self.inner.append_on_prefix(from_idx, entries).inspect(|_| {
            self.append_idx = from_idx + len;
        })
    }

    fn set_promise(
        &mut self,
        n_prom: omnipaxos::ballot_leader_election::Ballot,
    ) -> omnipaxos::storage::StorageResult<()> {
        self.inner.set_promise(n_prom)
    }

    fn set_decided_idx(&mut self, ld: usize) -> omnipaxos::storage::StorageResult<()> {
        self.inner.set_decided_idx(ld)
    }

    fn get_decided_idx(&self) -> omnipaxos::storage::StorageResult<usize> {
        self.inner.get_decided_idx()
    }

    fn set_accepted_round(
        &mut self,
        na: omnipaxos::ballot_leader_election::Ballot,
    ) -> omnipaxos::storage::StorageResult<()> {
        self.inner.set_accepted_round(na)
    }

    fn get_accepted_round(
        &self,
    ) -> omnipaxos::storage::StorageResult<Option<omnipaxos::ballot_leader_election::Ballot>> {
        self.inner.get_accepted_round()
    }

    fn get_entries(&self, from: usize, to: usize) -> omnipaxos::storage::StorageResult<Vec<Entry>> {
        self.inner.get_entries(from, to)
    }

    fn get_log_len(&self) -> omnipaxos::storage::StorageResult<usize> {
        self.inner.get_log_len()
    }

    fn get_suffix(&self, from: usize) -> omnipaxos::storage::StorageResult<Vec<Entry>> {
        self.inner.get_suffix(from)
    }

    fn get_promise(&self) -> omnipaxos::storage::StorageResult<Option<omnipaxos::ballot_leader_election::Ballot>> {
        self.inner.get_promise()
    }

    fn set_stopsign(&mut self, s: Option<omnipaxos::storage::StopSign>) -> omnipaxos::storage::StorageResult<()> {
        self.inner.set_stopsign(s)
    }

    fn get_stopsign(&self) -> omnipaxos::storage::StorageResult<Option<omnipaxos::storage::StopSign>> {
        self.inner.get_stopsign()
    }

    fn trim(&mut self, idx: usize) -> omnipaxos::storage::StorageResult<()> {
        todo!()
    }

    fn set_compacted_idx(&mut self, idx: usize) -> omnipaxos::storage::StorageResult<()> {
        todo!()
    }

    fn get_compacted_idx(&self) -> omnipaxos::storage::StorageResult<usize> {
        todo!()
    }

    fn set_snapshot(
        &mut self,
        snapshot: Option<<Entry as omnipaxos::storage::Entry>::Snapshot>,
    ) -> omnipaxos::storage::StorageResult<()> {
        todo!()
    }

    fn get_snapshot(
        &self,
    ) -> omnipaxos::storage::StorageResult<Option<<Entry as omnipaxos::storage::Entry>::Snapshot>> {
        todo!()
    }
}
