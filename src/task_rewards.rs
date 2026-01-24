// Task Rewards Module - User Task Incentives + Payment Ledger + Merkle Distributor + Claim Ticket API
// 
// This module implements a complete task incentive system with:
// - Task contract management
// - User task state tracking
// - Payment ledger
// - Merkle tree snapshot generation for reward distribution
// - Claim ticket generation for Solana on-chain claims
//
// Merkle Tree Specification (CRITICAL - Must match Solana contract):
// Leaf: SHA256(epoch_u64_le || index_u64_le || wallet_pubkey_32bytes || amount_u64_le)
// Node: SHA256(min(left, right) || max(left, right)) - sorted for direction-free proofs

use candid::{CandidType, Deserialize, Principal};
use ic_stable_structures::{Storable, storable::Bound};
use std::borrow::Cow;
use serde::Serialize;
use sha2::{Sha256, Digest};

// ===== Data Structures =====

/// Task contract item - defines a task and its reward
#[derive(CandidType, Deserialize, Serialize, Clone, Debug)]
pub struct TaskContractItem {
    pub taskid: String,
    pub reward: u64,  // PMUG tokens (smallest unit)
    pub payfor: Option<String>,  // Optional: link to payment event (e.g., "ai_subscription")
}

impl Storable for TaskContractItem {
    fn to_bytes(&self) -> Cow<[u8]> {
        let bytes = bincode::serialize(self).expect("Failed to serialize TaskContractItem");
        Cow::Owned(bytes)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        bincode::deserialize(&bytes).expect("Failed to deserialize TaskContractItem")
    }

    const BOUND: Bound = Bound::Unbounded;
}

/// Task status enum
#[derive(CandidType, Deserialize, Serialize, Clone, Debug, PartialEq)]
pub enum TaskStatus {
    NotStarted,
    InProgress,
    Completed,
    RewardPrepared,  // Added to epoch snapshot, waiting for claim
    TicketIssued,    // Ticket generated, waiting for on-chain claim
    Claimed,         // Successfully claimed on-chain
}

/// Claim result status (must match `aio-base-backend.did`)
#[derive(CandidType, Deserialize, Serialize, Clone, Debug, PartialEq)]
pub enum ClaimResultStatus {
    Success,
    Failed,
}

/// User task detail
#[derive(CandidType, Deserialize, Serialize, Clone, Debug)]
pub struct UserTaskDetail {
    pub taskid: String,
    pub status: TaskStatus,
    // Candid must match `aio-base-backend.did`: nat64 (use 0 when not completed)
    pub completed_at: u64,
    pub reward_amount: u64,
    pub evidence: Option<String>,
}

/// User task state - aggregates all tasks for a wallet
#[derive(CandidType, Deserialize, Serialize, Clone, Debug)]
pub struct UserTaskState {
    pub wallet: String,  // Solana wallet address (base58)
    pub tasks: Vec<UserTaskDetail>,
    // Candid must match `aio-base-backend.did`: total_unclaimed nat64
    pub total_unclaimed: u64,
}

// ---- Stable storage backward compatibility ----
// We used bincode for stable storage. Older versions stored different shapes.
// To avoid breaking upgrades, we attempt to decode the new shape first, then fall back to old.
#[derive(Deserialize)]
struct OldUserTaskDetail {
    taskid: String,
    status: TaskStatus,
    completed_at: Option<u64>,
    reward_amount: u64,
    evidence: Option<String>,
    prepared_epoch: Option<u64>,
}

#[derive(Deserialize)]
struct OldUserTaskState {
    wallet: String,
    tasks: Vec<OldUserTaskDetail>,
    updated_at: u64,
}

impl Storable for UserTaskState {
    fn to_bytes(&self) -> Cow<[u8]> {
        let bytes = bincode::serialize(self).expect("Failed to serialize UserTaskState");
        Cow::Owned(bytes)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        // Try new shape first
        if let Ok(v) = bincode::deserialize::<UserTaskState>(&bytes) {
            return v;
        }

        // Fall back to old shape and convert
        let old: OldUserTaskState =
            bincode::deserialize(&bytes).expect("Failed to deserialize UserTaskState (old)");

        let tasks: Vec<UserTaskDetail> = old
            .tasks
            .into_iter()
            .map(|t| UserTaskDetail {
                taskid: t.taskid,
                status: t.status,
                completed_at: t.completed_at.unwrap_or(0),
                reward_amount: t.reward_amount,
                evidence: t.evidence,
            })
            .collect();

        let total_unclaimed = compute_total_unclaimed(&tasks);

        UserTaskState {
            wallet: old.wallet,
            tasks,
            total_unclaimed,
        }
    }

    const BOUND: Bound = Bound::Unbounded;
}

fn compute_total_unclaimed(tasks: &[UserTaskDetail]) -> u64 {
    tasks
        .iter()
        .filter(|t| t.status != TaskStatus::Claimed)
        .filter(|t| matches!(t.status, TaskStatus::RewardPrepared | TaskStatus::TicketIssued))
        .map(|t| t.reward_amount)
        .sum()
}

/// Payment record
#[derive(CandidType, Deserialize, Serialize, Clone, Debug)]
pub struct PaymentRecord {
    pub wallet: String,
    pub amount_paid: u64,
    pub tx_ref: String,  // Transaction reference (order ID, payment ID, or blockchain tx)
    pub ts: u64,
    pub payfor: Option<String>,  // e.g., "ai_subscription", "voice_clone"
}

impl Storable for PaymentRecord {
    fn to_bytes(&self) -> Cow<[u8]> {
        let bytes = bincode::serialize(self).expect("Failed to serialize PaymentRecord");
        Cow::Owned(bytes)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        bincode::deserialize(&bytes).expect("Failed to deserialize PaymentRecord")
    }

    const BOUND: Bound = Bound::Unbounded;
}

/// Claimable entry - represents a leaf in the Merkle tree
#[derive(CandidType, Deserialize, Serialize, Clone, Debug)]
pub struct ClaimEntry {
    pub epoch: u64,
    pub index: u64,
    pub wallet: String,  // Solana pubkey base58
    pub amount: u64,     // PMUG smallest unit
}

impl Storable for ClaimEntry {
    fn to_bytes(&self) -> Cow<[u8]> {
        let bytes = bincode::serialize(self).expect("Failed to serialize ClaimEntry");
        Cow::Owned(bytes)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        bincode::deserialize(&bytes).expect("Failed to deserialize ClaimEntry")
    }

    const BOUND: Bound = Bound::Unbounded;
}

/// Merkle snapshot metadata
#[derive(CandidType, Deserialize, Serialize, Clone, Debug)]
pub struct MerkleSnapshotMeta {
    pub epoch: u64,
    pub root: [u8; 32],
    pub leaves_count: u64,
    pub locked: bool,
    pub created_at: u64,
}

impl Storable for MerkleSnapshotMeta {
    fn to_bytes(&self) -> Cow<[u8]> {
        let bytes = bincode::serialize(self).expect("Failed to serialize MerkleSnapshotMeta");
        Cow::Owned(bytes)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        bincode::deserialize(&bytes).expect("Failed to deserialize MerkleSnapshotMeta")
    }

    const BOUND: Bound = Bound::Unbounded;
}

/// Claim ticket - returned to frontend for on-chain claim
#[derive(CandidType, Deserialize, Serialize, Clone, Debug)]
pub struct ClaimTicket {
    pub epoch: u64,
    pub index: u64,
    pub wallet: String,
    pub amount: u64,
    pub proof: Vec<Vec<u8>>,  // Changed from Vec<[u8;32]> for Candid compatibility
    pub root: Vec<u8>,        // Changed from [u8;32] for Candid compatibility
}

/// Layer offset info for efficient Merkle tree storage
#[derive(CandidType, Deserialize, Serialize, Clone, Debug)]
pub struct LayerOffset {
    pub start: u64,
    pub len: u32,
}

impl Storable for LayerOffset {
    fn to_bytes(&self) -> Cow<[u8]> {
        let bytes = bincode::serialize(self).expect("Failed to serialize LayerOffset");
        Cow::Owned(bytes)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        bincode::deserialize(&bytes).expect("Failed to deserialize LayerOffset")
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 16,  // u64 + u32 with overhead
        is_fixed_size: false,
    };
}

/// Merkle hash node (32 bytes)
#[derive(CandidType, Deserialize, Serialize, Clone, Debug)]
pub struct MerkleHash(pub [u8; 32]);

impl Storable for MerkleHash {
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Borrowed(&self.0)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        MerkleHash(arr)
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 32,
        is_fixed_size: true,
    };
}

/// Key for epoch wallet index
#[derive(CandidType, Deserialize, Serialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EpochWalletKey {
    pub epoch: u64,
    pub wallet: String,
}

impl Storable for EpochWalletKey {
    fn to_bytes(&self) -> Cow<[u8]> {
        let bytes = bincode::serialize(self).expect("Failed to serialize EpochWalletKey");
        Cow::Owned(bytes)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        bincode::deserialize(&bytes).expect("Failed to deserialize EpochWalletKey")
    }

    const BOUND: Bound = Bound::Unbounded;
}

/// Key for epoch layer offsets
#[derive(CandidType, Deserialize, Serialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EpochLayerKey {
    pub epoch: u64,
    pub layer_id: u32,
}

impl Storable for EpochLayerKey {
    fn to_bytes(&self) -> Cow<[u8]> {
        let bytes = bincode::serialize(self).expect("Failed to serialize EpochLayerKey");
        Cow::Owned(bytes)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        bincode::deserialize(&bytes).expect("Failed to deserialize EpochLayerKey")
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 16, // u64 + u32 + overhead
        is_fixed_size: false,
    };
}

// ===== Merkle Tree Functions =====

/// Compute leaf hash according to specification:
/// SHA256(epoch || index || wallet_pubkey || amount)
/// All values in little-endian format
fn compute_leaf_hash(epoch: u64, index: u64, wallet_bytes: &[u8], amount: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(&epoch.to_le_bytes());
    // Use 4 bytes for index to match Solana u32
    hasher.update(&(index as u32).to_le_bytes());
    hasher.update(wallet_bytes);
    hasher.update(&amount.to_le_bytes());
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

/// Compute parent hash with sorted children (direction-free)
fn compute_parent_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    if left <= right {
        hasher.update(left);
        hasher.update(right);
    } else {
        hasher.update(right);
        hasher.update(left);
    }
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

/// Decode base58 Solana wallet address to 32 bytes
fn decode_wallet_base58(wallet: &str) -> Result<[u8; 32], String> {
    let decoded = bs58::decode(wallet)
        .into_vec()
        .map_err(|e| format!("Invalid base58: {}", e))?;
    
    if decoded.len() != 32 {
        return Err(format!("Invalid wallet length: expected 32 bytes, got {}", decoded.len()));
    }
    
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&decoded);
    Ok(bytes)
}

// ===== Storage Access Functions =====

use crate::stable_mem_storage::{
    TASK_CONTRACT,
    USER_TASKS,
    PAYMENTS,
    EPOCH_META,
    EPOCH_WALLET_INDEX,
    EPOCH_LAYERS,
    EPOCH_LAYER_OFFSETS,
};

/// Initialize task contract with default tasks
pub fn init_task_contract(tasks: Vec<TaskContractItem>) -> Result<(), String> {
    // Verify admin permission
    let caller = ic_cdk::caller();
    if !ic_cdk::api::is_controller(&caller) {
        return Err("Only controller can initialize task contract".to_string());
    }

    TASK_CONTRACT.with(|store| {
        let mut map = store.borrow_mut();
        for task in tasks {
            ic_cdk::println!("Initializing task: {} with reward: {}", task.taskid, task.reward);
            map.insert(task.taskid.clone(), task);
        }
    });

    Ok(())
}

/// Get task contract
pub fn get_task_contract() -> Vec<TaskContractItem> {
    TASK_CONTRACT.with(|store| {
        let map = store.borrow();
        map.iter().map(|(_, v)| v.clone()).collect()
    })
}

/// Get or initialize user tasks
pub fn get_or_init_user_tasks(wallet: String) -> UserTaskState {
    // Validate wallet format
    if let Err(e) = decode_wallet_base58(&wallet) {
        ic_cdk::println!("Warning: Invalid wallet format: {}", e);
    }

    USER_TASKS.with(|store| {
        let mut map = store.borrow_mut();
        
        if let Some(state) = map.get(&wallet) {
            return state.clone();
        }

        // Initialize new user tasks from contract
        let tasks: Vec<UserTaskDetail> = TASK_CONTRACT.with(|contract_store| {
            let contract = contract_store.borrow();
            contract.iter()
                .map(|(_, item)| UserTaskDetail {
                    taskid: item.taskid.clone(),
                    status: TaskStatus::NotStarted,
                    completed_at: 0,
                    reward_amount: item.reward,
                    evidence: None,
                })
                .collect()
        });

        let total_unclaimed = compute_total_unclaimed(&tasks);

        let state = UserTaskState {
            wallet: wallet.clone(),
            tasks,
            total_unclaimed,
        };

        map.insert(wallet, state.clone());
        state
    })
}

/// Record payment and auto-complete related task if payfor matches
pub fn record_payment(
    wallet: String,
    amount_paid: u64,
    tx_ref: String,
    ts: u64,
    payfor: Option<String>,
) -> Result<(), String> {
    // Validate wallet
    decode_wallet_base58(&wallet)?;

    // Create payment record
    let payment = PaymentRecord {
        wallet: wallet.clone(),
        amount_paid,
        tx_ref: tx_ref.clone(),
        ts,
        payfor: payfor.clone(),
    };

    // Store payment
    let payment_id = PAYMENTS.with(|store| {
        let vec = store.borrow_mut();
        let id = vec.len();
        vec.push(&payment).map_err(|e| format!("Failed to store payment: {:?}", e))?;
        Ok::<u64, String>(id)
    })?;

    ic_cdk::println!("Recorded payment {} for wallet {}: {} paid for {:?}", payment_id, wallet, amount_paid, payfor);

    // If payfor is specified, try to auto-complete matching task
    if let Some(payfor_str) = payfor {
        // Check if there's a task in contract matching this payfor
        let matching_task = TASK_CONTRACT.with(|store| {
            store.borrow()
                .iter()
                .find(|(_, item)| item.payfor.as_ref().map_or(false, |pf| pf == &payfor_str))
                .map(|(taskid, _)| taskid.clone())
        });

        if let Some(taskid) = matching_task {
            // 先检查用户任务是否存在，如果不存在则初始化（避免双重借用）
            let user_exists = USER_TASKS.with(|store| {
                store.borrow().contains_key(&wallet)
            });
            
            if !user_exists {
                // 如果用户不存在，先初始化（在借用外部）
                get_or_init_user_tasks(wallet.clone());
            }
            
            // 现在更新用户任务
            USER_TASKS.with(|store| {
                let mut map = store.borrow_mut();
                let mut state = map.get(&wallet)
                    .expect("User state should exist after initialization")
                    .clone();

                // Find and complete the matching task
                for task in &mut state.tasks {
                    if task.taskid == taskid && (task.status == TaskStatus::NotStarted || task.status == TaskStatus::InProgress) {
                        task.status = TaskStatus::Completed;
                        task.completed_at = ts;
                        ic_cdk::println!("Auto-completed task {} for wallet {} via payment", taskid, wallet);
                        break;
                    }
                }

                state.total_unclaimed = compute_total_unclaimed(&state.tasks);
                map.insert(wallet, state);
            });
        }
    }

    Ok(())
}

/// Complete a task
pub fn complete_task(
    wallet: String,
    taskid: String,
    evidence: Option<String>,
    ts: u64,
) -> Result<(), String> {
    // Validate wallet
    decode_wallet_base58(&wallet)?;

    // Verify task exists
    let task_contract = TASK_CONTRACT.with(|store| {
        store.borrow()
            .get(&taskid)
            .ok_or_else(|| format!("Task {} not found in contract", taskid))
    })?;

    // Update user task
    // 先检查用户任务是否存在，如果不存在则初始化（避免双重借用）
    let user_exists = USER_TASKS.with(|store| {
        store.borrow().contains_key(&wallet)
    });
    
    if !user_exists {
        // 如果用户不存在，先初始化（在借用外部）
        get_or_init_user_tasks(wallet.clone());
    }
    
    // 现在更新用户任务
    USER_TASKS.with(|store| {
        let mut map = store.borrow_mut();
        let mut state = map.get(&wallet)
            .ok_or_else(|| format!("User state not found for wallet {}", wallet))?
            .clone();

        // Find and complete the task
        let task_found = state.tasks.iter_mut()
            .find(|t| t.taskid == taskid)
            .map(|task| {
                if task.status == TaskStatus::NotStarted || task.status == TaskStatus::InProgress {
                    task.status = TaskStatus::Completed;
                    task.completed_at = ts;
                    task.reward_amount = task_contract.reward;
                    task.evidence = evidence.clone();
                    ic_cdk::println!("Completed task {} for wallet {}", taskid, wallet);
                    true
                } else {
                    false
                }
            })
            .unwrap_or(false);

        if !task_found {
            return Err(format!("Task {} not found or already completed for wallet", taskid));
        }

        state.total_unclaimed = compute_total_unclaimed(&state.tasks);
        map.insert(wallet, state);
        Ok(())
    })
}

/// Build epoch snapshot - generates Merkle tree and freezes claimable rewards
pub fn build_epoch_snapshot(epoch: u64) -> Result<MerkleSnapshotMeta, String> {
    // Verify admin permission
    let caller = ic_cdk::caller();
    if !ic_cdk::api::is_controller(&caller) {
        return Err("Only controller can build epoch snapshot".to_string());
    }

    // Check if epoch already exists
    let exists = EPOCH_META.with(|store| {
        store.borrow().contains_key(&epoch)
    });

    if exists {
        return Err(format!("Epoch {} snapshot already exists", epoch));
    }

    // Collect all completed tasks that haven't been prepared for an epoch
    let mut entries: Vec<ClaimEntry> = Vec::new();
    
    USER_TASKS.with(|store| {
        let map = store.borrow();
        for (wallet, state) in map.iter() {
            let mut total_amount = 0u64;
            
            for task in &state.tasks {
                // Only include tasks that are completed but not yet prepared/claimed
                if task.status == TaskStatus::Completed {
                    total_amount += task.reward_amount;
                }
            }
            
            if total_amount > 0 {
                entries.push(ClaimEntry {
                    epoch,
                    index: 0,  // Will be set after sorting
                    wallet: wallet.clone(),
                    amount: total_amount,
                });
            }
        }
    });

    if entries.is_empty() {
        return Err("No claimable rewards found for this epoch".to_string());
    }

    // Sort by wallet address (deterministic ordering)
    entries.sort_by(|a, b| a.wallet.cmp(&b.wallet));
    
    // Assign indices
    for (idx, entry) in entries.iter_mut().enumerate() {
        entry.index = idx as u64;
    }

    ic_cdk::println!("Building Merkle tree for epoch {} with {} entries", epoch, entries.len());

    // Compute leaf hashes
    let mut current_layer: Vec<[u8; 32]> = Vec::new();
    for entry in &entries {
        let wallet_bytes = decode_wallet_base58(&entry.wallet)?;
        let leaf_hash = compute_leaf_hash(entry.epoch, entry.index, &wallet_bytes, entry.amount);
        current_layer.push(leaf_hash);
    }

    // Store layer 0 (leaves)
    let mut all_layers: Vec<Vec<[u8; 32]>> = vec![current_layer.clone()];

    // Build tree layers
    while current_layer.len() > 1 {
        let mut next_layer = Vec::new();
        
        for chunk in current_layer.chunks(2) {
            if chunk.len() == 2 {
                let parent = compute_parent_hash(&chunk[0], &chunk[1]);
                next_layer.push(parent);
            } else {
                // Odd number: duplicate the last hash
                let parent = compute_parent_hash(&chunk[0], &chunk[0]);
                next_layer.push(parent);
            }
        }
        
        all_layers.push(next_layer.clone());
        current_layer = next_layer;
    }

    let root = current_layer[0];
    ic_cdk::println!("Merkle root for epoch {}: {:?}", epoch, root);

    // Store layers in flat structure
    EPOCH_LAYERS.with(|store| {
        let vec = store.borrow_mut();
        let base_offset = vec.len();
        
        // Store all hashes
        for layer in &all_layers {
            for hash in layer {
                vec.push(&MerkleHash(*hash))
                    .map_err(|e| format!("Failed to store Merkle hash: {:?}", e))?;
            }
        }

        // Store layer offsets
        let mut offset = base_offset;
        for (layer_id, layer) in all_layers.iter().enumerate() {
            let layer_offset = LayerOffset {
                start: offset,
                len: layer.len() as u32,
            };
            
            EPOCH_LAYER_OFFSETS.with(|offset_store| {
                offset_store.borrow_mut().insert(
                    EpochLayerKey { epoch, layer_id: layer_id as u32 },
                    layer_offset
                );
            });
            
            offset += layer.len() as u64;
        }

        Ok::<(), String>(())
    })?;

    // Store wallet -> (index, amount) mapping
    EPOCH_WALLET_INDEX.with(|store| {
        let mut map = store.borrow_mut();
        for entry in &entries {
            map.insert(
                EpochWalletKey { epoch, wallet: entry.wallet.clone() },
                (entry.index, entry.amount)
            );
        }
    });

    // Update user tasks to RewardPrepared status
    USER_TASKS.with(|store| {
        let mut map = store.borrow_mut();
        for entry in &entries {
            if let Some(mut state) = map.get(&entry.wallet) {
                for task in &mut state.tasks {
                    if task.status == TaskStatus::Completed {
                        task.status = TaskStatus::RewardPrepared;
                    }
                }
                state.total_unclaimed = compute_total_unclaimed(&state.tasks);
                map.insert(entry.wallet.clone(), state);
            }
        }
    });

    // Store metadata
    let meta = MerkleSnapshotMeta {
        epoch,
        root,
        leaves_count: entries.len() as u64,
        locked: true,
        created_at: ic_cdk::api::time(),
    };

    EPOCH_META.with(|store| {
        store.borrow_mut().insert(epoch, meta.clone());
    });

    ic_cdk::println!("Successfully built epoch {} snapshot with {} leaves", epoch, entries.len());
    Ok(meta)
}

/// Get claim ticket for a wallet
pub fn get_claim_ticket(wallet: String) -> Result<ClaimTicket, String> {
    // Validate wallet
    decode_wallet_base58(&wallet)?;

    // Find the latest epoch where this wallet has claimable rewards
    let (epoch, index, amount) = EPOCH_WALLET_INDEX.with(|store| {
        let map = store.borrow();
        
        // Find all epochs for this wallet
        let mut epochs: Vec<(u64, u64, u64)> = Vec::new();
        for (key, (idx, amt)) in map.iter() {
            if key.wallet == wallet {
                epochs.push((key.epoch, idx, amt));
            }
        }
        
        if epochs.is_empty() {
            return Err("No claimable rewards found for this wallet".to_string());
        }
        
        // Sort by epoch descending and take the latest
        epochs.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(epochs[0])
    })?;

    // Check if ticket was already issued
    let already_issued = USER_TASKS.with(|store| {
        let map = store.borrow();
        if let Some(state) = map.get(&wallet) {
            state.tasks.iter().any(|t| 
                (t.status == TaskStatus::TicketIssued || t.status == TaskStatus::Claimed)
            )
        } else {
            false
        }
    });

    if already_issued {
        return Err("Ticket already issued for this epoch".to_string());
    }

    // Get root from metadata
    let root = EPOCH_META.with(|store| {
        store.borrow()
            .get(&epoch)
            .map(|meta| meta.root)
            .ok_or_else(|| format!("Epoch {} metadata not found", epoch))
    })?;

    // Generate proof
    let proof = generate_merkle_proof(epoch, index)?;

    // Mark as ticket issued
    USER_TASKS.with(|store| {
        let mut map = store.borrow_mut();
        if let Some(mut state) = map.get(&wallet) {
            for task in &mut state.tasks {
                if task.status == TaskStatus::RewardPrepared {
                    task.status = TaskStatus::TicketIssued;
                }
            }
            state.total_unclaimed = compute_total_unclaimed(&state.tasks);
            map.insert(wallet.clone(), state);
        }
    });

    Ok(ClaimTicket {
        epoch,
        index: index as u64,
        wallet,
        amount,
        proof: proof.iter().map(|h| h.to_vec()).collect(),
        root: root.to_vec(),
    })
}

/// Generate Merkle proof for a given leaf index
fn generate_merkle_proof(epoch: u64, leaf_index: u64) -> Result<Vec<[u8; 32]>, String> {
    let mut proof = Vec::new();
    let mut current_index = leaf_index as usize;

    // Get total number of layers
    let max_layer = EPOCH_LAYER_OFFSETS.with(|store| {
        let map = store.borrow();
        let mut max = 0u32;
        for (key, _) in map.iter() {
            if key.epoch == epoch && key.layer_id > max {
                max = key.layer_id;
            }
        }
        max
    });

    // Traverse from leaf to root (excluding root itself)
    for layer_id in 0..max_layer {
        // Get sibling index
        let sibling_index = if current_index % 2 == 0 {
            current_index + 1
        } else {
            current_index - 1
        };

        // Get layer offset
        let layer_offset = EPOCH_LAYER_OFFSETS.with(|store| {
            store.borrow()
                .get(&EpochLayerKey { epoch, layer_id })
                .ok_or_else(|| format!("Layer offset not found for epoch {} layer {}", epoch, layer_id))
        })?;

        // Read sibling hash
        // If the layer has an odd number of nodes and the current node is the last one,
        // the sibling is the node itself (duplicate for hashing)
        let hash_position = if (sibling_index as u32) < layer_offset.len {
            layer_offset.start + sibling_index as u64
        } else {
            layer_offset.start + current_index as u64
        };

        let sibling_hash = EPOCH_LAYERS.with(|store| {
            store.borrow()
                .get(hash_position)
                .map(|h| h.0)
                .ok_or_else(|| format!("Hash not found at position {}", hash_position))
        })?;
        
        proof.push(sibling_hash);

        // Move to parent index
        current_index /= 2;
    }

    Ok(proof)
}

/// Mark claim result (callback from frontend after on-chain claim)
pub fn mark_claim_result(
    wallet: String,
    epoch: u64,
    status: ClaimResultStatus,
    tx_sig: Option<String>,
) -> Result<(), String> {
    // Validate wallet
    decode_wallet_base58(&wallet)?;

    USER_TASKS.with(|store| {
        let mut map = store.borrow_mut();
        let mut state = map.get(&wallet)
            .ok_or_else(|| format!("User state not found for wallet {}", wallet))?;

        let updated = match status {
            ClaimResultStatus::Success => {
                // Mark as claimed
                for task in &mut state.tasks {
                    if task.status == TaskStatus::TicketIssued {
                        task.status = TaskStatus::Claimed;
                    }
                }
                ic_cdk::println!("Marked epoch {} as claimed for wallet {} (tx: {:?})", epoch, wallet, tx_sig);
                true
            },
            ClaimResultStatus::Failed => {
                // Revert to RewardPrepared to allow retry
                for task in &mut state.tasks {
                    if task.status == TaskStatus::TicketIssued {
                        task.status = TaskStatus::RewardPrepared;
                    }
                }
                ic_cdk::println!("Reverted epoch {} to RewardPrepared for wallet {} (failed)", epoch, wallet);
                true
            },
        };

        if updated {
            state.total_unclaimed = compute_total_unclaimed(&state.tasks);
            map.insert(wallet, state);
        }

        Ok(())
    })
}

/// Get epoch metadata
pub fn get_epoch_meta(epoch: u64) -> Option<MerkleSnapshotMeta> {
    EPOCH_META.with(|store| {
        store.borrow().get(&epoch)
    })
}

/// List all epoch metadata
pub fn list_all_epochs() -> Vec<MerkleSnapshotMeta> {
    EPOCH_META.with(|store| {
        store.borrow().iter().map(|(_, v)| v).collect()
    })
}
