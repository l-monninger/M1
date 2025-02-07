//! Manages the virtual machine states.
use std::{
    collections::HashMap,
    io::{self, Error, ErrorKind},
    sync::Arc,
};
use avalanche_types::{choices, ids, subnet};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use crate::util::types::block::Block;

/// Manages block and chain states for this Vm, both in-memory and persistent.
#[derive(Clone)]
pub struct State {
    /// Maps block Id to Block.
    /// Each element is verified but not yet accepted/rejected (e.g., preferred).
    /// This should be a fast in-memory structure, whereas the db can be disk.
    pub verified_blocks: Arc<RwLock<HashMap<ids::Id, Block>>>,

    // Database for persistent storage of VM state.
    // This is where we store blocks once they are accepted.
    pub db: Arc<RwLock<Box<dyn subnet::rpc::database::Database + Send + Sync>>>,
    // [ verified_blocks ] -> [ db ]

    pub preferred : ids::Id,


}

impl Default for State {
    fn default() -> State {
        Self {
            db: Arc::new(RwLock::new(subnet::rpc::database::memdb::Database::new())),
            verified_blocks: Arc::new(RwLock::new(HashMap::new())),
            preferred : ids::Id::empty(),
        }
    }
}

const LAST_ACCEPTED_BLOCK_KEY: &[u8] = b"last_accepted_block";

const STATUS_PREFIX: u8 = 0x0;

const DELIMITER: u8 = b'/';

/// Returns a vec of bytes used as a key for identifying blocks in state.
/// 'STATUS_PREFIX' + 'BYTE_DELIMITER' + [block_id]
fn block_with_status_key(blk_id: &ids::Id) -> Vec<u8> {
    let mut k: Vec<u8> = Vec::with_capacity(ids::LEN + 2);
    k.push(STATUS_PREFIX);
    k.push(DELIMITER);
    k.extend_from_slice(&blk_id.to_vec());
    k
}

/// Wraps a [`Block`](crate::block::Block) and its status.
/// This is the data format that [`State`](State) uses to persist blocks.
#[derive(Serialize, Deserialize, Clone)]
struct BlockWithStatus {
    block_bytes: Vec<u8>,
    status: choices::status::Status,
}

impl BlockWithStatus {
    fn encode(&self) -> io::Result<Vec<u8>> {
        serde_json::to_vec(&self).map_err(|e| {
            Error::new(
                ErrorKind::Other,
                format!("failed to serialize BlockStatus to JSON bytes: {}", e),
            )
        })
    }

    fn from_slice(d: impl AsRef<[u8]>) -> io::Result<Self> {
        let dd = d.as_ref();
        serde_json::from_slice(dd).map_err(|e| {
            Error::new(
                ErrorKind::Other,
                format!("failed to deserialize BlockStatus from JSON: {}", e),
            )
        })
    }
}

pub enum VerificationStatus {
    Genesis,
    Verified,
    AlreadyAdded,
    InvalidBlockHeight,
    TimestampGreaterThanParent,
    TimestampGreaterThanLocal,
}

impl State {

    /// Persists the last accepted block Id to state.
    pub async fn set_last_accepted_block(&self, blk_id: &ids::Id) -> io::Result<()> {
        let mut db = self.db.write().await;
        db.put(LAST_ACCEPTED_BLOCK_KEY, &blk_id.to_vec())
            .await
            .map_err(|e| {
                Error::new(
                    ErrorKind::Other,
                    format!("failed to put last accepted block: {:?}", e),
                )
            })
    }

    /// Returns "true" if there's a last accepted block found.
    pub async fn has_last_accepted_block(&self) -> io::Result<bool> {
        let db = self.db.read().await;
        match db.has(LAST_ACCEPTED_BLOCK_KEY).await {
            Ok(found) => Ok(found),
            Err(e) => Err(Error::new(
                ErrorKind::Other,
                format!("failed to load last accepted block: {}", e),
            )),
        }
    }

    /// Returns the last accepted block Id from state.
    pub async fn get_last_accepted_block_id(&self) -> io::Result<ids::Id> {
        let db = self.db.read().await;
        match db.get(LAST_ACCEPTED_BLOCK_KEY).await {
            Ok(d) => Ok(ids::Id::from_slice(&d)),
            Err(e) => {
                if subnet::rpc::errors::is_not_found(&e) {
                    return Ok(ids::Id::empty());
                }
                Err(e)
            }
        }
    }

    /// Adds a block to "verified_blocks".
    pub async fn add_verified(&mut self, block: &Block) {
        let blk_id = block.id();
        let mut verified_blocks = self.verified_blocks.write().await;
        verified_blocks.insert(blk_id, block.clone());
    }

    /// Removes a block from "verified_blocks".
    pub async fn remove_verified(&mut self, blk_id: &ids::Id) {
        let mut verified_blocks = self.verified_blocks.write().await;
        verified_blocks.remove(blk_id);
    }

    /// Returns "true" if the block Id has been already verified.
    pub async fn has_verified(&self, blk_id: &ids::Id) -> bool {
        let verified_blocks = self.verified_blocks.read().await;
        verified_blocks.contains_key(blk_id)
    }

    /// Writes a block to the state storage.
    pub async fn write_block(&mut self, block: &Block) -> io::Result<()> {
        let blk_id = block.id();
        let blk_bytes = block.to_slice()?;

        let mut db = self.db.write().await;

        let blk_status = BlockWithStatus {
            block_bytes: blk_bytes,
            status: block.status(),
        };
        let blk_status_bytes = blk_status.encode()?;

        db.put(&block_with_status_key(&blk_id), &blk_status_bytes)
            .await
            .map_err(|e| Error::new(ErrorKind::Other, format!("failed to put block: {:?}", e)))
    }

    /// Reads a block from the state storage using the block_with_status_key.
    pub async fn get_block(&self, blk_id: &ids::Id) -> io::Result<Block> {
        // check if the block exists in memory as previously verified.
        let verified_blocks = self.verified_blocks.read().await;
        if let Some(b) = verified_blocks.get(blk_id) {
            return Ok(b.clone());
        }
        let db = self.db.read().await;

        let blk_status_bytes = db.get(&block_with_status_key(blk_id)).await?;
        let blk_status = BlockWithStatus::from_slice(blk_status_bytes)?;

        let mut blk = Block::from_slice(&blk_status.block_bytes)?;
        blk.set_status(blk_status.status);

        Ok(blk)
    }

    pub async fn verify_block(&self, block: &Block) -> io::Result<VerificationStatus, anyhow::Error> {

        // todo: double check that blindly accepting genesis block does not causes issues
        if block.height() == 0 && block.parent_id() == ids::Id::empty() {
            return Ok(VerificationStatus::Genesis);
        }

        // if already exists in database, it means it's already accepted
        // thus no need to verify once more
        if self.get_block(&block.id()).await.is_ok() {
            return Ok(VerificationStatus::AlreadyAdded);
        }


        let parent_blk = self.get_block(&block.parent_id()).await?;
        // ensure the height of the block is immediately following its parent
        if parent_blk.height() != block.height() - 1 {
            return Ok(VerificationStatus::InvalidBlockHeight);
        }

        // ensure block timestamp is after its parent
        if parent_blk.timestamp() > block.timestamp() {
            return Ok(VerificationStatus::TimestampGreaterThanLocal);
        }

        // ensure block timestamp is no more than an hour ahead of this nodes time
        if block.timestamp() >= (Utc::now() + Duration::hours(1)).timestamp() as u64 {
            return Ok(VerificationStatus::TimestampGreaterThanLocal);
        }

        Ok(VerificationStatus::Verified)
        
    }

    // Set preferred
    pub async fn set_preferred(&mut self, blk_id: &ids::Id) -> io::Result<()> {
        self.preferred = blk_id.clone();
        Ok(())
    }

    // Accept block should only accept a fully built block
    pub async fn accept_block(&mut self, block: &Block) -> io::Result<()> {
        block.set_status(status::Status::Accepted);
        self.write_block(block).await?;
        self.set_last_accepted_block(&block.id()).await?;
        self.remove_verified(blk_id).await;
    }

    // Reject block should write a block to the db with a rejected status
    pub async fn reject_block(&mut self, block : &Block) -> io::Result<()> {
        block.set_status(status::Status::Rejected);
        self.write_block(block).await?; // blocks are written to the db when rejected for further rejection
        self.remove_verified(blk_id).await;
    }

}