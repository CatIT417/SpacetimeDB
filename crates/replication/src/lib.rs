use omnipaxos::macros::Entry as OmniPaxosEntry;
use serde::{Deserialize, Serialize};
use storage::MetaDataLog;

mod commitlog;
mod node;
mod storage;

#[derive(Debug, Clone, OmniPaxosEntry, Serialize, Deserialize)]
pub struct Entry {
    pub crc: u32,
}

pub enum CommitLogInstruction {
    FromReplication(MetaDataLog),
    FromDB(u32),
}
