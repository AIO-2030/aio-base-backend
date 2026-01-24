use candid::{CandidType, Principal, Decode, Encode};
use ic_cdk::api::time;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use crate::stable_mem_storage::{MINING_REWARD_POLICY, REWARD_ENTRIES, USER_REWARD_INDEX, MCP_REWARD_INDEX};
use crate::token_economy_types::RewardIdList;
use ic_stable_structures::storable::Bound;
use std::borrow::Cow;

// Quarterly reward configuration
#[derive(CandidType, Clone, Debug, Serialize, Deserialize)]
pub struct QuarterRewardConfig {
    pub quarter: u32,
    pub base_reward: u64,
    pub estimated_calls: u64,
    pub total_quarter_emission: u64,
}

// Global mining policy
#[derive(CandidType, Clone, Debug, Serialize, Deserialize)]
pub struct MiningRewardPolicy {
    pub start_epoch_sec: u64,
    pub decay_rate: f32,
    pub total_emission_cap: u64,
    pub quarters: Vec<QuarterRewardConfig>,
}

// Kappa tier structure
#[derive(CandidType, Clone, Debug, Serialize, Deserialize)]
pub struct KappaTier {
    pub stake_ratio_threshold: f32,
    pub kappa_multiplier: f32,
}

// Stake record
#[derive(CandidType, Clone, Debug, Serialize, Deserialize)]
pub struct StakeRecord {
    pub principal_id: Principal,
    pub mcp_name: String,
    pub stack_amount: u64,
}

// Reward entry
#[derive(CandidType, Clone, Debug, Serialize, Deserialize)]
pub struct RewardEntry {
    pub principal_id: Principal,
    pub mcp_name: String,
    pub reward_amount: u64,
    pub block_id: u64,
    pub status: String,
}

// Call record
#[derive(CandidType, Clone, Debug, Serialize, Deserialize)]
pub struct CallRecord {
    pub block_id: u64,
    pub mcp_name: String,
    pub quality_score: f32,
}

// User reward key for indexing
#[derive(CandidType, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct UserRewardKey {
    pub principal_id: Principal,
    pub mcp_name: String,
}

// Implement Storable for QuarterRewardConfig
impl ic_stable_structures::Storable for QuarterRewardConfig {
    fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
        Cow::Owned(Encode!(self).expect("Failed to encode QuarterRewardConfig"))
    }

    fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).expect("Failed to decode QuarterRewardConfig")
    }

    const BOUND: Bound = Bound::Bounded { max_size: 1024 * 32, is_fixed_size: false };
}

// Implement Storable for MiningRewardPolicy
impl ic_stable_structures::Storable for MiningRewardPolicy {
    fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
        Cow::Owned(Encode!(self).expect("Failed to encode MiningRewardPolicy"))
    }

    fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).expect("Failed to decode MiningRewardPolicy")
    }

    const BOUND: Bound = Bound::Bounded { max_size: 1024 * 32, is_fixed_size: false };
}

// Implement Storable for RewardEntry
impl ic_stable_structures::Storable for RewardEntry {
    fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
        Cow::Owned(Encode!(self).expect("Failed to encode RewardEntry"))
    }

    fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).expect("Failed to decode RewardEntry")
    }

    const BOUND: Bound = Bound::Bounded { max_size: 1024 * 32, is_fixed_size: false };
}

// Implement Storable for UserRewardKey
impl ic_stable_structures::Storable for UserRewardKey {
    fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
        Cow::Owned(Encode!(self).expect("Failed to encode UserRewardKey"))
    }

    fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).expect("Failed to decode UserRewardKey")
    }

    const BOUND: Bound = Bound::Bounded { max_size: 1024 * 32, is_fixed_size: false };
}

// Default mining configuration
pub fn default_mining_config() -> MiningRewardPolicy {
    let mut quarters = Vec::new();
    let mut base_reward = 300_000u64;
    let mut estimated_calls = 1000u64;
    let decay_rate = 0.045f32; // 4.5% decay per quarter
    
    for quarter in 1..=40 {
        let total_quarter_emission = base_reward * estimated_calls;
        quarters.push(QuarterRewardConfig {
            quarter,
            base_reward,
            estimated_calls,
            total_quarter_emission,
        });
        
        // Update values for next quarter
        base_reward = (base_reward as f32 * (1.0 - decay_rate)) as u64;
        estimated_calls = (estimated_calls as f32 * 1.2) as u64; // 20% growth
    }

    MiningRewardPolicy {
        start_epoch_sec: time(),
        decay_rate,
        total_emission_cap: 8_400_000_000_000_000u64, // 8.4 Quadrillion
        quarters,
    }
}

// Default Kappa tiers
pub fn default_kappa_tiers() -> Vec<KappaTier> {
    vec![
        KappaTier { stake_ratio_threshold: 0.01, kappa_multiplier: 1.0 },
        KappaTier { stake_ratio_threshold: 0.05, kappa_multiplier: 1.1 },
        KappaTier { stake_ratio_threshold: 0.10, kappa_multiplier: 1.3 },
        KappaTier { stake_ratio_threshold: 0.25, kappa_multiplier: 1.5 },
        KappaTier { stake_ratio_threshold: 0.50, kappa_multiplier: 1.7 },
        KappaTier { stake_ratio_threshold: 0.75, kappa_multiplier: 1.85 },
        KappaTier { stake_ratio_threshold: 1.00, kappa_multiplier: 2.0 },
    ]
}

// Initialize mining policy
pub fn init_mining_policy() {
    let policy = default_mining_config();
    MINING_REWARD_POLICY.with(|p| {
        p.borrow_mut().insert("default".to_string(), policy);
    });
}

// Calculate user rewards
pub fn calculate_user_rewards_for_mcp(
    mcp_name: String,
    calls: Vec<CallRecord>,
    stakes: Vec<StakeRecord>,
    current_quarter: u32,
) -> Vec<RewardEntry> {
    let mut reward_entries = Vec::new();
    let mut next_reward_id = 0u64;
    
    // Get base reward for current quarter
    let policy = MINING_REWARD_POLICY.with(|p| {
        let policy_store = p.borrow();
        if policy_store.is_empty() {
            // Initialize policy if empty
            drop(policy_store); // Release the borrow before mutating
            init_mining_policy();
            p.borrow().get(&"default".to_string()).unwrap()
        } else {
            policy_store.get(&"default".to_string()).unwrap_or_else(default_mining_config)
        }
    });
    
    let quarter_config = policy.quarters
        .iter()
        .find(|q| q.quarter == current_quarter)
        .expect("Invalid quarter");
    
    // Calculate total stake for MCP
    let total_stake: u64 = stakes.iter()
        .filter(|s| s.mcp_name == mcp_name)
        .map(|s| s.stack_amount)
        .sum();
    
    // Process each call record
    for call in calls.iter().filter(|c| c.mcp_name == mcp_name) {
        for stake in stakes.iter().filter(|s| s.mcp_name == mcp_name) {
            let stake_ratio = stake.stack_amount as f32 / total_stake as f32;
            
            // Determine kappa multiplier
            let kappa = default_kappa_tiers()
                .iter()
                .rev()
                .find(|tier| stake_ratio >= tier.stake_ratio_threshold)
                .map(|tier| tier.kappa_multiplier)
                .unwrap_or(1.0);
            
            // Calculate reward
            let reward = (quarter_config.base_reward as f32 * kappa * call.quality_score) as u64;
            
            let reward_entry = RewardEntry {
                principal_id: stake.principal_id,
                mcp_name: mcp_name.clone(),
                reward_amount: reward,
                block_id: call.block_id,
                status: "pending".to_string(),
            };
            
            // Store in stable memory
            REWARD_ENTRIES.with(|entries| {
                entries.borrow_mut().insert(next_reward_id, reward_entry.clone());
            });
            
            // Update user reward index
            let user_key = UserRewardKey {
                principal_id: stake.principal_id,
                mcp_name: mcp_name.clone(),
            };
            USER_REWARD_INDEX.with(|index| {
                let mut reward_ids = index.borrow()
                    .get(&user_key)
                    .map(|list| list.0)
                    .unwrap_or_default();
                reward_ids.push(next_reward_id);
                index.borrow_mut().insert(user_key, RewardIdList(reward_ids));
            });
            
            // Update MCP reward index
            MCP_REWARD_INDEX.with(|index| {
                let mut reward_ids = index.borrow()
                    .get(&mcp_name)
                    .map(|list| list.0)
                    .unwrap_or_default();
                reward_ids.push(next_reward_id);
                index.borrow_mut().insert(mcp_name.clone(), RewardIdList(reward_ids));
            });
            
            reward_entries.push(reward_entry);
            next_reward_id += 1;
        }
    }
    
    reward_entries
}

// Query interfaces
pub fn get_mining_policy() -> MiningRewardPolicy {
    MINING_REWARD_POLICY.with(|p| {
        p.borrow()
            .get(&"default".to_string())
            .unwrap_or_else(default_mining_config)
    })
}

pub fn get_kappa_tiers() -> Vec<KappaTier> {
    default_kappa_tiers()
}

pub fn get_pending_rewards(principal: Principal) -> Vec<RewardEntry> {
    let mut pending_rewards = Vec::new();
    
    // Get all reward IDs for the user
    let reward_ids = USER_REWARD_INDEX.with(|index| {
        let mut ids = Vec::new();
        for (key, list) in index.borrow().iter() {
            if key.principal_id == principal {
                ids.extend(list.0.clone());
            }
        }
        ids
    });
    
    // Get reward entries
    REWARD_ENTRIES.with(|entries| {
        for id in reward_ids {
            if let Some(entry) = entries.borrow().get(&id) {
                if entry.status == "pending" {
                    pending_rewards.push(entry.clone());
                }
            }
        }
    });
    
    pending_rewards
}

// Get rewards for specific MCP with pagination
pub fn get_mcp_rewards_paginated(mcp_name: String, offset: u64, limit: u64) -> Vec<RewardEntry> {
    let mut mcp_rewards = Vec::new();
    
    // Get all reward IDs for the MCP
    let reward_ids = MCP_REWARD_INDEX.with(|index| {
        index.borrow()
            .get(&mcp_name)
            .map(|list| list.0)
            .unwrap_or_default()
    });
    
    // Calculate pagination range
    let start = offset as usize;
    let end = std::cmp::min((offset + limit) as usize, reward_ids.len());
    
    // Get reward entries for the paginated range
    REWARD_ENTRIES.with(|entries| {
        for id in reward_ids.iter().skip(start).take(end - start) {
            if let Some(entry) = entries.borrow().get(id) {
                mcp_rewards.push(entry.clone());
            }
        }
    });
    
    mcp_rewards
}

// Get rewards for specific MCP
pub fn get_mcp_rewards(mcp_name: String) -> Vec<RewardEntry> {
    let mut all_rewards = Vec::new();
    let mut offset = 0u64;
    let limit = 100u64; // Process 100 records per page
    let mut has_more = true;
    
    while has_more {
        let page_rewards = get_mcp_rewards_paginated(mcp_name.clone(), offset, limit);
        if page_rewards.is_empty() {
            has_more = false;
            continue;
        }
        
        all_rewards.extend(page_rewards);
        offset += limit;
    }
    
    all_rewards
}

// Calculate quality score based on stake ratio
fn calculate_quality_score(stake_ratio: f32) -> f32 {
    match stake_ratio {
        r if r < 0.01 => 1.0,    // < 1%
        r if r < 0.05 => 1.1,    // 1% - 5%
        r if r < 0.10 => 1.3,    // 5% - 10%
        r if r < 0.25 => 1.5,    // 10% - 25%
        r if r < 0.50 => 1.7,    // 25% - 50%
        r if r < 0.75 => 1.85,   // 50% - 75%
        _ => 2.0,                // > 75%
    }
}

// Predict mining rewards for all pages
pub fn perdic_mining() -> Result<Vec<RewardEntry>, String> {
    ic_cdk::println!("[perdic_mining] Starting mining reward prediction");
    let mut all_reward_entries = Vec::new();
    let mut offset = 0u64;
    let limit = 100u64; // Process 100 records per page
    let mut has_more = true;
    
    // Get current quarter
    let current_quarter = 1u32; // This should be calculated based on actual time
    ic_cdk::println!("[perdic_mining] Current quarter: {}", current_quarter);
    
    while has_more {
        ic_cdk::println!("[perdic_mining] Processing page with offset: {}", offset);
        // Get paginated TraceItems
        let trace_items = crate::trace_storage::get_traces_for_mining_days(offset, limit);
        if trace_items.is_empty() {
            ic_cdk::println!("[perdic_mining] No more trace items found, ending pagination");
            has_more = false;
            continue;
        }
        ic_cdk::println!("[perdic_mining] Retrieved {} trace items", trace_items.len());
        
        // Group by MCP name
        let mut mcp_traces: HashMap<String, Vec<&crate::trace_storage::TraceItem>> = HashMap::new();
        for item in &trace_items {
            // Only process unclaimed traces
            if item.status != "claimed" {
                mcp_traces.entry(item.agent.clone())
                    .or_insert_with(Vec::new)
                    .push(item);
            }
        }
        ic_cdk::println!("[perdic_mining] Grouped traces by {} MCPs", mcp_traces.len());
        
        // Process rewards for each MCP
        for (mcp_name, traces) in mcp_traces {
            ic_cdk::println!("[perdic_mining] Processing MCP: {}, with {} traces", mcp_name, traces.len());
            // Get all stack records for this MCP
            let stack_records = get_all_mcp_stack_records(mcp_name.clone());
            
            if stack_records.is_empty() {
                ic_cdk::println!("[perdic_mining] No stack records found for MCP: {}", mcp_name);
                continue;
            }
            ic_cdk::println!("[perdic_mining] Found {} stack records for MCP: {}", stack_records.len(), mcp_name);
            
            // Calculate total stake
            let total_stake: u64 = stack_records.iter()
                .filter(|r| matches!(r.stack_status, crate::mcp_asset_types::StackStatus::Stacked))
                .map(|r| r.stack_amount)
                .sum();
            
            if total_stake == 0 {
                ic_cdk::println!("[perdic_mining] Total stake is 0 for MCP: {}", mcp_name);
                continue;
            }
            ic_cdk::println!("[perdic_mining] Total stake for MCP {}: {}", mcp_name, total_stake);
            
            // Get mining policy
            let policy = get_mining_policy();
            let quarter_config = policy.quarters
                .iter()
                .find(|q| q.quarter == current_quarter)
                .ok_or_else(|| format!("Invalid quarter configuration: {}", current_quarter))?;
            ic_cdk::println!("[perdic_mining] Using base reward: {} for quarter {}", quarter_config.base_reward, current_quarter);
            
            // Calculate rewards for each stack record
            for stack_record in stack_records {
                if !matches!(stack_record.stack_status, crate::mcp_asset_types::StackStatus::Stacked) {
                    continue;
                }
                
                let stake_ratio = stack_record.stack_amount as f32 / total_stake as f32;
                let quality_score = calculate_quality_score(stake_ratio);
                ic_cdk::println!("[perdic_mining] Stack record - Principal: {}, Stake ratio: {}, Quality score: {}", 
                    stack_record.principal_id, stake_ratio, quality_score);
                
                // Calculate reward for each trace
                for trace in &traces {
                    // Recheck trace status
                    if trace.status != "ok" {
                        continue;
                    }

                    let principal_id = candid::Principal::from_text(&stack_record.principal_id)
                        .unwrap_or_else(|_| candid::Principal::anonymous());

                    let reward = (quarter_config.base_reward as f32 * quality_score) as u64;
                    ic_cdk::println!("[perdic_mining] Calculating reward for trace {} - Amount: {}", trace.trace_id, reward);
                    
                    let reward_entry = RewardEntry {
                        principal_id,
                        mcp_name: mcp_name.clone(),
                        reward_amount: reward,
                        block_id: trace.timestamp,
                        status: "pending".to_string(),
                    };
                    
                    // Store reward record
                    let next_id = REWARD_ENTRIES.with(|entries| {
                        let mut entries = entries.borrow_mut();
                        let next_id = entries.len() as u64;
                        entries.insert(next_id, reward_entry.clone());
                        next_id
                    });
                    ic_cdk::println!("[perdic_mining] Stored reward entry with ID: {}", next_id);

                    // Update user reward index
                    let user_key = UserRewardKey {
                        principal_id,
                        mcp_name: mcp_name.clone(),
                    };
                    
                    USER_REWARD_INDEX.with(|index| {
                        let mut reward_ids = index.borrow()
                            .get(&user_key)
                            .map(|list| list.0)
                            .unwrap_or_default();
                        reward_ids.push(next_id);
                        index.borrow_mut().insert(user_key, RewardIdList(reward_ids));
                    });
                    
                    // Update MCP reward index
                    MCP_REWARD_INDEX.with(|index| {
                        let mut reward_ids = index.borrow()
                            .get(&mcp_name)
                            .map(|list| list.0)
                            .unwrap_or_default();
                        reward_ids.push(next_id);
                        index.borrow_mut().insert(mcp_name.clone(), RewardIdList(reward_ids));
                    });

                    // Update trace status to claimed
                    if let Err(e) = crate::trace_storage::update_trace_status(
                        trace.trace_id.clone(),
                        "claimed".to_string()
                    ) {
                        ic_cdk::println!("[perdic_mining] Failed to update trace status: {}", e);
                        return Err(format!("Failed to update trace status: {}", e));
                    }
                    ic_cdk::println!("[perdic_mining] Updated trace status to claimed for trace: {}", trace.trace_id);
                    
                    all_reward_entries.push(reward_entry);
                }
            }
        }
        
        offset += limit;
    }
    
    ic_cdk::println!("[perdic_mining] Completed mining reward prediction. Total reward entries: {}", all_reward_entries.len());
    Ok(all_reward_entries)
}

// Get all stack records for a specific MCP
fn get_all_mcp_stack_records(mcp_name: String) -> Vec<crate::mcp_asset_types::McpStackRecord> {
    let mut all_records = Vec::new();
    let mut offset = 0u64;
    let limit = 100u64;
    let mut has_more = true;

    while has_more {
        let records = crate::mcp_asset_types::get_mcp_stack_records_paginated(
            mcp_name.clone(),
            offset,
            limit
        );

        if records.is_empty() {
            has_more = false;
            continue;
        }

        all_records.extend(records);
        offset += limit;
    }

    all_records
}

// Calculate total unclaimed rewards for a principal
pub fn cal_unclaim_rewards(principal: Principal) -> u64 {
    let mut total_rewards = 0u64;
    
    // Get all reward IDs for the user
    let reward_ids = USER_REWARD_INDEX.with(|index| {
        let mut ids = Vec::new();
        for (key, list) in index.borrow().iter() {
            if key.principal_id == principal {
                ids.extend(list.0.clone());
            }
        }
        ids
    });
    
    // Sum up all pending rewards
    REWARD_ENTRIES.with(|entries| {
        for id in reward_ids {
            if let Some(entry) = entries.borrow().get(&id) {
                if entry.status == "pending" {
                    total_rewards += entry.reward_amount;
                }
            }
        }
    });
    
    total_rewards
}

// Claim rewards for a principal
pub async fn claim_rewards(principal: Principal) -> Result<u64, String> {
    // 1. Get all pending rewards for the user
    let pending_rewards = get_pending_rewards(principal);
    if pending_rewards.is_empty() {
        return Err("No pending rewards to claim".to_string());
    }

    // 2. Calculate total reward amount
    let total_amount: u64 = pending_rewards.iter()
        .map(|entry| entry.reward_amount)
        .sum();

    // 3. Call ICRC2 transfer_from contract for token transfer
    let ledger_canister_id = Principal::from_text(crate::token_economy_types::TOKEN_LEDGER_CANISTER_ID)
        .map_err(|e| format!("Invalid ledger canister ID: {}", e))?;

    let user_account = Account {
        owner: principal,
        subaccount: None,
    };
    let mining_pool_account = Account {
        owner: Principal::from_text(crate::token_economy_types::AIO_MINING_POOL_ID)
            .map_err(|e| format!("Invalid mining pool principal: {}", e))?,
        subaccount: None,
    };
    
    let transfer_args = TransferFromArgs {
        spender_subaccount: None,
        from: mining_pool_account,
        to: user_account,
        amount: candid::Nat::from(total_amount),
        fee: None,
        memo: None,
        created_at_time: Some(ic_cdk::api::time()),
    };

    #[derive(CandidType, Deserialize)]
    enum TransferFromResult {
        Ok(candid::Nat),
        Err(TransferFromError),
    }

    let result = ic_cdk::call::<(TransferFromArgs,), (TransferFromResult,)>(
        ledger_canister_id,
        "icrc2_transfer_from",
        (transfer_args,)
    ).await;

    match result {
        Ok((TransferFromResult::Ok(_block_height),)) => {
            // 4. Update all reward record statuses to claimed
            let reward_ids: Vec<u64> = REWARD_ENTRIES.with(|entries| {
                let entries = entries.borrow();
                entries.iter()
                    .filter(|(_, entry)| entry.principal_id == principal && entry.status == "pending")
                    .map(|(id, _)| id.clone())
                    .collect()
            });

            
            REWARD_ENTRIES.with(|entries| {
                let mut entries = entries.borrow_mut();
                for id in reward_ids {
                    if let Some(entry) = entries.get(&id) {
                        let mut updated_entry = entry.clone();
                        updated_entry.status = "claimed".to_string();
                        entries.insert(id, updated_entry);
                    }
                }
            });

            Ok(total_amount)
        },
        Ok((TransferFromResult::Err(e),)) => Err(format!("Transfer failed: {:?}", e)),
        Err((code, msg)) => Err(format!("Canister call failed: {:?} - {}", code, msg)),
    }
}

// Add necessary types for ICRC2 transfer
#[derive(CandidType, Clone, Debug)]
pub struct Account {
    pub owner: Principal,
    pub subaccount: Option<Vec<u8>>,
}

#[derive(CandidType, Clone, Debug)]
pub struct TransferFromArgs {
    pub spender_subaccount: Option<Vec<u8>>,
    pub from: Account,
    pub to: Account,
    pub amount: candid::Nat,
    pub fee: Option<candid::Nat>,
    pub memo: Option<Vec<u8>>,
    pub created_at_time: Option<u64>,
}

#[derive(CandidType, Clone, Debug, Deserialize)]
pub enum TransferFromError {
    BadFee { expected_fee: candid::Nat },
    BadBurn { min_burn_amount: candid::Nat },
    InsufficientFunds { balance: candid::Nat },
    InsufficientAllowance { allowance: candid::Nat },
    TooOld,
    CreatedInFuture { ledger_time: u64 },
    Duplicate { duplicate_of: candid::Nat },
    TemporarilyUnavailable,
    GenericError { error_code: candid::Nat, message: String },
}

// Get total amount of all reward entries regardless of status
pub fn get_total_aiotoken_claimable() -> u64 {
    let mut total_amount = 0u64;
    
    REWARD_ENTRIES.with(|entries| {
        for (_, entry) in entries.borrow().iter() {
            total_amount += entry.reward_amount;
        }
    });
    
    total_amount
}

// Get all MCP rewards with pagination
pub fn get_all_mcp_rewards_paginated(offset: u64, limit: u64) -> Vec<RewardEntry> {
    let mut all_rewards = Vec::new();
    let mut processed_count = 0u64;
    
    // Get all MCP names from MCP_REWARD_INDEX
    let mcp_names: Vec<String> = MCP_REWARD_INDEX.with(|index| {
        index.borrow()
            .iter()
            .map(|(mcp_name, _)| mcp_name.clone())
            .collect()
    });
    
    // Iterate through all MCPs
    for mcp_name in mcp_names {
        // Get reward IDs for current MCP
        let reward_ids = MCP_REWARD_INDEX.with(|index| {
            index.borrow()
                .get(&mcp_name)
                .map(|list| list.0)
                .unwrap_or_default()
        });
        
        // Process rewards for current MCP
        REWARD_ENTRIES.with(|entries| {
            for id in reward_ids {
                if processed_count >= offset && processed_count < offset + limit {
                    if let Some(entry) = entries.borrow().get(&id) {
                        all_rewards.push(entry.clone());
                    }
                }
                processed_count += 1;
                
                // Break if we've reached the limit
                if all_rewards.len() >= limit as usize {
                    break;
                }
            }
        });
        
        // Break if we've reached the limit
        if all_rewards.len() >= limit as usize {
            break;
        }
    }
    
    all_rewards
}

// Get total count of all MCP rewards
pub fn get_total_mcp_rewards_count() -> u64 {
    let mut total_count = 0u64;
    
    MCP_REWARD_INDEX.with(|index| {
        for (_, list) in index.borrow().iter() {
            total_count += list.0.len() as u64;
        }
    });
    
    total_count
}
