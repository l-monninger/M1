use async_trait::async_trait;

// Top-level definition of traits.
// Complex extensions and integrations should be defined in the submodules.
#[async_trait]
pub trait SequencerLayer {

    type Transaction;
    type TransactionId;
    
    // Receives a transaction and internally sends it on to the next layer.
    async fn receive_transaction(
        &self,
        transaction: Self::Transaction
    ) -> Result<(), anyhow::Error>;

    // Gets a received transaction.
    async fn get_transaction(
        &self,
        transaction_id: Self::TransactionId
    ) -> Result<Option<Self::Transaction>, anyhow::Error>;

}

#[async_trait]
pub trait ProposerLayer {

    type Transaction;
    type Block;
    type BlockId;

    // Gets the next transaction from the previous layer.
    async fn get_next_transaction(
        &self
    ) -> Result<Option<Self::Transaction>, anyhow::Error>;

    // Constructs a block from some transactions
    async fn build_block(
        &self,
    ) -> Result<Self::Block, anyhow::Error>;

    // Sends a constructed block to the next layer.
    async fn send_block(
        &self,
        block: Self::Block
    ) -> Result<(), anyhow::Error>;

    // Gets a constructed and sent block
    async fn get_block(
        &self,
        block_id: Self::BlockId
    ) -> Result<Option<Self::Block>, anyhow::Error>;


}

#[async_trait]
pub trait DataAvailabilityLayer {

    type Block;
    type BlockId;

    // Gets the next block from the previous layer.
    async fn get_next_block(
        &self
    ) -> Result<Option<Self::Block>, anyhow::Error>;

    // Sends a block to the next layer or place retrievable from the next layer, i.e., the execution layer.
    async fn send_block(
        &self,
        block: Self::Block
    ) -> Result<(), anyhow::Error>;

    // Gets a block that was sent to the next layer.
    async fn get_block(
        &self,
        block_id: Self::BlockId
    ) -> Result<Option<Self::Block>, anyhow::Error>;

}

#[async_trait]
pub trait ExecutionLayer {

    type Block;
    type BlockId;
    type ChangeSet;

    // Gets the next block from the previous layer.
    async fn get_next_block(
        &self
    ) -> Result<Option<Self::Block>, anyhow::Error>;

    // Executes a block and produces a change set.
    async fn execute_block(
        &self,
        block: Self::Block
    ) -> Result<ChangeSet, anyhow::Error>;

    // Sends a change set to the next layer,  i.e., the storage layer.
    async fn send_change_set(
        &self,
        change_set: ChangeSet
    ) -> Result<(), anyhow::Error>;

    // Gets an executed block
    async fn get_block(
        &self,
        block_id: Self::BlockId
    ) -> Result<Option<Self::Block>, anyhow::Error>;

}

#[async_trait]
pub trait StorageLayer {

    type Block;
    type BlockId;
    type ChangeSet;
    type StateEntry;
    type Address;

    // Gets the next change set from the previous layer.
    async fn get_next_change_set(
        &self
    ) -> Result<Option<Self::ChangeSet>, anyhow::Error>;

    // Applies a change set to the storage layer.
    async fn derive_state(
        &self,
        change_set: Self::ChangeSet
    ) -> Result<(), anyhow::Error>;

    // Gets a state entry from the storage layer.
    async fn get_state_entry(
        &self,
        address: Self::Address
    ) -> Result<Option<Self::StateEntry>, anyhow::Error>;

    // Gets an applied change set
    async fn get_change_set(
        &self,
        block_id: Self::BlockId
    ) -> Result<Option<Self::ChangeSet>, anyhow::Error>;

}

#[async_trait]
pub trait SettlementLayer {

    type Block;
    type BlockId;
    type Commitment;

    // Gets the next block from the previous layer
    async fn get_next_block(
        &self
    ) -> Result<Option<Self::Block>, anyhow::Error>;

    // Gets a commitment from the previous layer
    async fn build_commitment(
        &self,
    ) -> Result<Self::Commitment, anyhow::Error>;

    // Applies a commitment to itself
    async fn apply_commitment(
        &self,
        commitment: Self::Commitment
    ) -> Result<(), anyhow::Error>;


}

#[async_trait]
pub trait MessagingLayer {

    type Message;

    // Sends a message to other layers.
    async fn send_message(
        &self,
        message: Self::Message
    ) -> Result<(), anyhow::Error>;

    // Receives a message from another layer and handles it internally.
    async fn receive_message(
        &self,
        message: Self::Message
    ) -> Result<(), anyhow::Error>;


}