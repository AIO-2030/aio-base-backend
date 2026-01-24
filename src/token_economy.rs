use candid::{CandidType, Principal, Decode, Encode};
use ic_cdk::api::time;
use ic_stable_structures::StableBTreeMap;
use std::cell::RefCell;
use num_traits::ToPrimitive;
use crate::token_economy_types::{
    EmissionPolicy, SubscriptionPlan, TokenGrant, TokenGrantKey,
    TokenActivity, TokenActivityType, CreditActivity, CreditActivityType,
    TransferStatus, AccountInfo, TokenInfo, TokenGrantStatus,
    NewMcpGrant, NewMcpGrantKey, GrantPolicy
};
use icrc_ledger_types::{icrc1::account::Account, icrc1::transfer::{TransferError, BlockIndex}};
use crate::trace_storage::{get_trace_by_id, record_trace_call, IOValue};
use crate::account_storage::{get_account, upsert_account};
use std::collections::HashMap;
use crate::token_economy_types::*;
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::DefaultMemoryImpl;
use std::borrow::Cow;
use serde::{Serialize, Deserialize};
use crate::mcp_asset_types;
use crate::stable_mem_storage::{NEWUSER_GRANTS, NEWMCP_GRANTS, TOKEN_ACTIVITIES, CREDIT_ACTIVITIES, EMISSION_POLICY, GRANT_POLICIES, CREDIT_CONVERT_CONTRACT, RECHARGE_RECORDS, RECHARGE_PRINCIPAL_ACCOUNTS};

// Re-export NumTokens for public use
pub use icrc_ledger_types::icrc1::transfer::NumTokens;

type Memory = VirtualMemory<DefaultMemoryImpl>;

// Constants
const EXCHANGE_RATIO: f64 = 1.0; // 1 AIO = 1 Credit
const STAKING_PERIOD: u64 = 30 * 24 * 60 * 60 * 1_000_000_000; // 30 days in nanoseconds
const MIN_STAKE_AMOUNT: u64 = 100; // Minimum amount of credits to stake
const MAX_KAPPA: f64 = 2.0; // Maximum kappa multiplier
const BASE_KAPPA: f64 = 1.0; // Base kappa multiplier
const DEFAULT_BASE_RATE: u64 = 100;
const DEFAULT_KAPPA_FACTOR: f64 = 1.0;
const DEFAULT_STAKING_BONUS: f64 = 0.1;
const ADMIN_PRINCIPAL: &str = "aaaaa-aa"; // TODO: Replace with actual admin Principal
const DEFAULT_ICP_USD_PRICE: f64 = 5.5;
const DEFAULT_CREDIT_USD_PRICE: f64 = 0.0001;
const CREDIT_CONTRACT_KEY: &str = "global";

// Account Management
pub async fn get_account_info(principal_id: String) -> Option<AccountInfo> {
    let mut account = get_account(principal_id.clone())?;
    
    // get token balance from ledger canisterƒ√
    let icrc_account = icrc_ledger_types::icrc1::account::Account {
        owner: candid::Principal::from_text(&principal_id).ok()?,
        subaccount: None,
    };

    let ledger_canister_id = candid::Principal::from_text(TOKEN_LEDGER_CANISTER_ID).ok()?;
    
    // Call ICRC1 balance_of method
    let result = ic_cdk::call::<(icrc_ledger_types::icrc1::account::Account,), (candid::Nat,)>(ledger_canister_id, "icrc1_balance_of", (icrc_account,)).await;
    
    match result {
        Ok((balance,)) => {
            // update account balance
            account.token_info.token_balance = balance.0.to_u64().unwrap_or(0);
            account.updated_at = Some(ic_cdk::api::time());
            
            // save updated account info
            upsert_account(account.clone()).ok()?;
            
            Some(account)
        },
        Err(_) => None
    }
}

pub fn create_account(principal_id: String) -> Result<AccountInfo, String> {
    let account = AccountInfo::new(principal_id);
    upsert_account(account)
}

pub fn update_account_balance(principal_id: String, token_amount: i64, credit_amount: i64) -> Result<AccountInfo, String> {
    let mut account = get_account(principal_id.clone())
        .ok_or_else(|| "Account not found".to_string())?;

    if token_amount < 0 && account.get_token_balance() < (-token_amount) as u64 {
        return Err("Insufficient token balance".to_string());
    }
    if credit_amount < 0 && account.get_credit_balance() < (-credit_amount) as u64 {
        return Err("Insufficient credit balance".to_string());
    }

    account.token_info.token_balance = (account.get_token_balance() as i64 + token_amount) as u64;
    account.token_info.credit_balance = (account.get_credit_balance() as i64 + credit_amount) as u64;
    account.updated_at = Some(time());

    upsert_account(account)
}

// Credit Operations
pub fn stack_credits(principal_id: String, mcp_name:String ,amount: u64) -> Result<AccountInfo, String> {
    if amount < MIN_STAKE_AMOUNT {
        return Err(format!("Minimum stake amount is {}", MIN_STAKE_AMOUNT));
    }

    let mut account = get_account(principal_id.clone())
        .ok_or_else(|| "Account not found".to_string())?;

    if account.get_credit_balance() < amount {
        return Err("Insufficient credit balance".to_string());
    }

    // Store original account state for potential rollback
    let original_account = account.clone();

    let new_credit_balance = account.get_credit_balance() - amount;
    let new_staked_credits = account.get_staked_credits() + amount;
    
    account.token_info.credit_balance = (new_credit_balance as i64) as u64;
    account.token_info.staked_credits = (new_staked_credits as i64) as u64;
    account.updated_at = Some(time());
    
    let result = upsert_account(account.clone())?;

    // Try to create stack record
    match mcp_asset_types::stack_mcp(
        mcp_name,
        principal_id.clone(),
        amount
    ) {
        Ok(_) => {
            // Record credit activity
            let activity = CreditActivity {
                timestamp: time(),
                principal_id: principal_id.clone(),
                amount,
                activity_type: CreditActivityType::Stack,
                status: TransferStatus::Completed,
                metadata: Some("Credit stacking".to_string()),
            };
            record_credit_activity(activity)?;

            Ok(result)
        },
        Err(e) => {
            // Rollback account changes
            upsert_account(original_account)?;
            Err(format!("Failed to create stack record: {}", e))
        }
    }
}

pub fn unstack_credits(principal_id: String, amount: u64) -> Result<AccountInfo, String> {
    let mut account = get_account(principal_id.clone())
        .ok_or_else(|| "Account not found".to_string())?;

    if account.get_staked_credits() < amount {
        return Err("Insufficient staked credits".to_string());
    }

    let new_staked_credits = account.get_staked_credits() - amount;
    let new_credit_balance = account.get_credit_balance() + amount;
    
    account.token_info.staked_credits = (new_staked_credits as i64) as u64;
    account.token_info.credit_balance = (new_credit_balance as i64) as u64;
    account.updated_at = Some(time());
    
    let result = upsert_account(account.clone())?;

    // Record credit activity
    let activity = CreditActivity {
        timestamp: time(),
        principal_id: principal_id.clone(),
        amount,
        activity_type: CreditActivityType::Unstack,
        status: TransferStatus::Completed,
        metadata: Some("Credit unstacking".to_string()),
    };
    record_credit_activity(activity)?;

    Ok(result)
}

// Token Operations
pub fn transfer_tokens(from: String, to: String, amount: u64) -> Result<AccountInfo, String> {
    let mut from_account = get_account(from.clone())
        .ok_or_else(|| "From account not found".to_string())?;
    
    let mut to_account = get_account(to.clone())
        .ok_or_else(|| "To account not found".to_string())?;
    
    if from_account.get_token_balance() < amount {
        return Err("Insufficient token balance".to_string());
    }

    let from_new_balance = from_account.get_token_balance() - amount;
    let to_new_balance = to_account.get_token_balance() + amount;
    
    from_account.token_info.token_balance = (from_new_balance as i64) as u64;
    to_account.token_info.token_balance = (to_new_balance as i64) as u64;
    
    from_account.updated_at = Some(time());
    to_account.updated_at = Some(time());
    
    upsert_account(from_account.clone())?;
    upsert_account(to_account.clone())?;
    
    // Record token activity
    let activity = TokenActivity {
        timestamp: time(),
        from: from.clone(),
        to: to.clone(),
        amount,
        activity_type: TokenActivityType::Transfer,
        status: TransferStatus::Completed,
        metadata: Some("Token transfer".to_string()),
    };
    record_token_activity(activity)?;
    
    Ok(from_account)
}

// Credit Usage
pub fn use_credits(principal_id: String, amount: u64, service: String, metadata: Option<String>) -> Result<AccountInfo, String> {
    let mut account = get_account(principal_id.clone())
        .ok_or_else(|| "Account not found".to_string())?;
    
    if account.get_credit_balance() < amount {
        return Err("Insufficient credit balance".to_string());
    }

    let new_credit_balance = account.get_credit_balance() - amount;
    account.token_info.credit_balance = (new_credit_balance as i64) as u64;
    account.updated_at = Some(time());
    
    let result = upsert_account(account.clone())?;
    
    // Record credit activity
    let activity = CreditActivity {
        timestamp: time(),
        principal_id: principal_id.clone(),
        amount,
        activity_type: CreditActivityType::Spend,
        status: TransferStatus::Completed,
        metadata: Some(format!("Credit usage for service: {} - {}", service, metadata.unwrap_or_default())),
    };
    record_credit_activity(activity)?;
    
    Ok(result)
}

// Token Grant Operations
pub fn create_token_grant(grant: TokenGrant) -> Result<(), String> {
    NEWUSER_GRANTS.with(|grants| {
        let key = TokenGrantKey {
            recipient: grant.recipient.clone(),
        };
        grants.borrow_mut().insert(key, grant);
        Ok(())
    })
}

pub fn claim_grant(principal_id: &str) -> Result<u64, String> {
    // First check if the account exists
    let account = get_account(principal_id.to_string())
        .ok_or_else(|| "Account not found".to_string())?;

    // Get the grant using the principal_id as recipient
    let grant = get_token_grant(principal_id)
        .ok_or_else(|| "No grant found for this account".to_string())?;

    let current_time = time();

    if current_time < grant.start_time {
        return Err("Grant period has not started".to_string());
    }

    let remaining_amount = grant.amount - grant.claimed_amount;
    if remaining_amount == 0 {
        return Err("No credits available to claim".to_string());
    }

    // Update grant
    let mut updated_grant = grant.clone();
    updated_grant.claimed_amount += remaining_amount;
    updated_grant.status = TokenGrantStatus::Active;
    create_token_grant(updated_grant)?;

    // Update account credit balance
    let mut account = account.clone();
    let new_credit_balance = account.get_credit_balance() + remaining_amount;
    account.token_info.credit_balance = (new_credit_balance as i64) as u64;
    account.updated_at = Some(current_time);
    ic_cdk::println!("Account updated: {:?}", account);
    upsert_account(account)?;

    // Record credit activity
    let activity = CreditActivity {
        timestamp: current_time,
        principal_id: principal_id.to_string(),
        amount: remaining_amount,
        activity_type: CreditActivityType::Earn,
        status: TransferStatus::Completed,
        metadata: Some("Grant credit claim".to_string()),
    };
    ic_cdk::println!("Record CreditActivity: {:?}", activity);
    record_credit_activity(activity)?;

    Ok(remaining_amount)
}

pub fn get_token_grant(recipient: &str) -> Option<TokenGrant> {
    NEWUSER_GRANTS.with(|grants| {
        let key = TokenGrantKey {
            recipient: recipient.to_string(),
        };
        grants.borrow().get(&key).map(|grant| grant.clone())
    })
}

pub fn get_all_token_grants() -> Vec<TokenGrant> {
    NEWUSER_GRANTS.with(|grants| {
        grants.borrow()
            .iter()
            .map(|(_, grant)| grant.clone())
            .collect()
    })
}

pub fn get_token_grants_paginated(offset: u64, limit: usize) -> Vec<TokenGrant> {
    NEWUSER_GRANTS.with(|grants| {
        grants.borrow()
            .iter()
            .skip(offset as usize)
            .take(limit)
            .map(|(_, grant)| grant.clone())
            .collect()
    })
}

pub fn get_token_grants_by_recipient(recipient: &str) -> Vec<TokenGrant> {
    NEWUSER_GRANTS.with(|grants| {
        grants.borrow()
            .iter()
            .filter(|(_, grant)| grant.recipient == recipient)
            .map(|(_, grant)| grant.clone())
            .collect()
    })
}

pub fn get_token_grants_by_status(status: &TokenGrantStatus) -> Vec<TokenGrant> {
    NEWUSER_GRANTS.with(|grants| {
        grants.borrow()
            .iter()
            .filter(|(_, grant)| grant.status == *status)
            .map(|(_, grant)| grant.clone())
            .collect()
    })
}

pub fn get_token_grants_count() -> u64 {
    NEWUSER_GRANTS.with(|grants| {
        grants.borrow().len() as u64
    })
}

// Activity Recording
pub fn record_token_activity(activity: TokenActivity) -> Result<(), String> {
    TOKEN_ACTIVITIES.with(|activities| {
        let mut activities = activities.borrow_mut();
        let index = activities.len();
        activities.insert(index, activity);
        Ok(())
    })
}

pub fn record_credit_activity(activity: CreditActivity) -> Result<(), String> {
    CREDIT_ACTIVITIES.with(|activities| {
        let mut activities = activities.borrow_mut();
        let index = activities.len();
        activities.insert(index, activity);
        Ok(())
    })
}

// Query Methods
pub fn get_account_token_info(principal_id: &str) -> Result<TokenInfo, String> {
    let account = get_account(principal_id.to_string())
        .ok_or_else(|| "Account not found".to_string())?;
    Ok(account.token_info)
}

pub fn get_balance_summary(principal_id: String) -> (u64, u64, u64, u64) {
    if let Some(account) = get_account(principal_id) {
        (
            account.get_token_balance(),
            account.get_staked_credits(),
            account.get_credit_balance(),
            0 // unclaimed_balance is not part of TokenInfo
        )
    } else {
        (0, 0, 0, 0)
    }
}

// Activity Query Methods
pub fn get_token_activities(principal_id: &str) -> Vec<TokenActivity> {
    TOKEN_ACTIVITIES.with(|activities| {
        activities.borrow()
            .iter()
            .filter(|(_, activity)| activity.from == principal_id || activity.to == principal_id)
            .map(|(_, activity)| activity.clone())
            .collect()
    })
}

pub fn get_credit_activities(principal_id: &str) -> Vec<CreditActivity> {
    CREDIT_ACTIVITIES.with(|activities| {
        activities.borrow()
            .iter()
            .filter(|(_, activity)| activity.principal_id == principal_id)
            .map(|(_, activity)| activity.clone())
            .collect()
    })
}

// Activity Statistics
pub fn get_token_activity_statistics(principal_id: &str) -> (u64, u64, u64) {
    let activities = get_token_activities(principal_id);
    let total_count = activities.len() as u64;
    let total_amount = activities.iter().map(|a| a.amount).sum();
    let success_count = activities.iter()
        .filter(|a| a.status == TransferStatus::Completed)
        .count() as u64;
    (total_count, total_amount, success_count)
}

pub fn get_credit_activity_statistics(principal_id: &str) -> (u64, u64, u64) {
    let activities = get_credit_activities(principal_id);
    let total_count = activities.len() as u64;
    let total_amount = activities.iter().map(|a| a.amount).sum();
    let success_count = activities.iter()
        .filter(|a| a.status == TransferStatus::Completed)
        .count() as u64;
    (total_count, total_amount, success_count)
}

// Emission Policy Operations
pub fn init_emission_policy() {
    let mut policy = EmissionPolicy {
        base_rate: DEFAULT_BASE_RATE,
        kappa_factor: DEFAULT_KAPPA_FACTOR,
        staking_bonus: DEFAULT_STAKING_BONUS,
        subscription_multipliers: HashMap::new(),
        last_update_time: time(),
    };

    // Set default subscription multipliers
    policy.subscription_multipliers.insert(SubscriptionPlan::Free, 1.0);
    policy.subscription_multipliers.insert(SubscriptionPlan::Basic, 1.5);
    policy.subscription_multipliers.insert(SubscriptionPlan::Premium, 2.0);
    policy.subscription_multipliers.insert(SubscriptionPlan::Enterprise, 3.0);

    EMISSION_POLICY.with(|p| {
        p.borrow_mut().insert("default".to_string(), policy);
    });
}
pub fn init_grant_policy(grant_policy: Option<GrantPolicy>) {
    let policies = match grant_policy {
        Some(policy) => vec![policy],
        None => vec![
            GrantPolicy {
                grant_amount: 1000, // 1000 credits for new users
                grant_action: GrantAction::NewUser,
                grant_duration: 0, //can be claimed once
            },
            GrantPolicy {
                grant_amount: 10000, // 10000 credits for new mcp registered
                grant_action: GrantAction::NewMcp,
                grant_duration: 0, // can be claimed once
            }
        ]
    };

    // Initialize all policies
    for policy in policies {
        GRANT_POLICIES.with(|policies| {
            let mut policies = policies.borrow_mut();
            policies.insert(policy.grant_action.clone(), policy);
        });
    }
}


pub fn calculate_emission(principal_id: &str) -> Result<u64, String> {
    let account = get_account(principal_id.to_string())
        .ok_or_else(|| "Account not found".to_string())?;
    let policy = get_emission_policy()?;
    
    let base_amount = policy.base_rate;
    let kappa_multiplier = account.get_kappa_multiplier();
    let staked_credits = account.get_staked_credits();
    let staking_bonus = if staked_credits > 0 {
        policy.staking_bonus
    } else {
        1.0
    };

    let subscription_multiplier = account.get_subscription_plan()
        .and_then(|plan| policy.subscription_multipliers.get(&plan))
        .copied()
        .unwrap_or(1.0);

    let emission = (base_amount as f64 * kappa_multiplier * staking_bonus * subscription_multiplier) as u64;
    Ok(emission)
}

pub fn get_emission_policy() -> Result<EmissionPolicy, String> {
    EMISSION_POLICY.with(|p| {
        p.borrow()
            .get(&"default".to_string())
            .ok_or_else(|| "Emission policy not found".to_string())
    })
}

pub fn update_emission_policy(policy: EmissionPolicy) -> Result<(), String> {
    EMISSION_POLICY.with(|p| {
        p.borrow_mut().insert("default".to_string(), policy);
        Ok(())
    })
}

// Activity Query Methods
pub fn get_token_activities_paginated(principal_id: &str, offset: u64, limit: usize) -> Vec<TokenActivity> {
    TOKEN_ACTIVITIES.with(|activities| {
        activities.borrow()
            .iter()
            .filter(|(_, activity)| activity.from == principal_id || activity.to == principal_id)
            .skip(offset as usize)
            .take(limit)
            .map(|(_, activity)| activity.clone())
            .collect()
    })
}

pub fn get_token_activities_by_type(principal_id: &str, activity_type: TokenActivityType) -> Vec<TokenActivity> {
    TOKEN_ACTIVITIES.with(|activities| {
        activities.borrow()
            .iter()
            .filter(|(_, activity)| 
                (activity.from == principal_id || activity.to == principal_id) && 
                activity.activity_type == activity_type
            )
            .map(|(_, activity)| activity.clone())
            .collect()
    })
}

pub fn get_token_activities_by_time_period(principal_id: &str, start_time: u64, end_time: u64) -> Vec<TokenActivity> {
    TOKEN_ACTIVITIES.with(|activities| {
        activities.borrow()
            .iter()
            .filter(|(_, activity)| 
                (activity.from == principal_id || activity.to == principal_id) && 
                activity.timestamp >= start_time && 
                activity.timestamp <= end_time
            )
            .map(|(_, activity)| activity.clone())
            .collect()
    })
}

pub fn get_credit_activities_paginated(principal_id: &str, offset: u64, limit: usize) -> Vec<CreditActivity> {
    CREDIT_ACTIVITIES.with(|activities| {
        activities.borrow()
            .iter()
            .filter(|(_, activity)| activity.principal_id == principal_id)
            .skip(offset as usize)
            .take(limit)
            .map(|(_, activity)| activity.clone())
            .collect()
    })
}

pub fn get_credit_activities_by_type(principal_id: &str, activity_type: CreditActivityType) -> Vec<CreditActivity> {
    CREDIT_ACTIVITIES.with(|activities| {
        activities.borrow()
            .iter()
            .filter(|(_, activity)| 
                activity.principal_id == principal_id && 
                activity.activity_type == activity_type
            )
            .map(|(_, activity)| activity.clone())
            .collect()
    })
}

pub fn get_credit_activities_by_time_period(principal_id: &str, start_time: u64, end_time: u64) -> Vec<CreditActivity> {
    CREDIT_ACTIVITIES.with(|activities| {
        activities.borrow()
            .iter()
            .filter(|(_, activity)| 
                activity.principal_id == principal_id && 
                activity.timestamp >= start_time && 
                activity.timestamp <= end_time
            )
            .map(|(_, activity)| activity.clone())
            .collect()
    })
}

// Credit Usage
pub fn log_credit_usage(principal_id: String, amount: u64, service: String, metadata: Option<String>) -> Result<(), String> {
    let activity = CreditActivity {
        timestamp: time(),
        principal_id: principal_id.clone(),
        amount,
        activity_type: CreditActivityType::Spend,
        status: TransferStatus::Completed,
        metadata: Some(format!("Credit usage for service: {} - {}", service, metadata.unwrap_or_default())),
    };
    record_credit_activity(activity)
}

// New MCP Grant Operations
pub fn create_mcp_grant(grant: NewMcpGrant) -> Result<(), String> {
    NEWMCP_GRANTS.with(|grants| {
        let key = NewMcpGrantKey {
            recipient: grant.recipient.clone(),
            mcp_name: grant.mcp_name.clone(),
        };
        grants.borrow_mut().insert(key, grant);
        Ok(())
    })
}

pub fn claim_mcp_grant(principal_id: &str) -> Result<u64, String> {
    // First check if the account exists
    let account = get_account(principal_id.to_string())
        .ok_or_else(|| "Account not found".to_string())?;

    // Get all MCP grants for this principal
    let grants = get_mcp_grants_by_recipient(principal_id);
    
    // Filter for active grants
    let active_grants: Vec<NewMcpGrant> = grants.into_iter()
        .filter(|grant| grant.status == TokenGrantStatus::Active)
        .collect();

    if active_grants.is_empty() {
        return Err("No active MCP grants found for this account".to_string());
    }

    let current_time = time();
    let mut total_claimed = 0;

    // Process each active grant
    for grant in active_grants {
        let remaining_amount = grant.amount - grant.claimed_amount;
        if remaining_amount == 0 {
            continue;
        }

        // Update grant status to completed
        let mut updated_grant = grant.clone();
        updated_grant.claimed_amount += remaining_amount;
        updated_grant.status = TokenGrantStatus::Completed;
        create_mcp_grant(updated_grant)?;

        total_claimed += remaining_amount;
    }

    if total_claimed == 0 {
        return Err("No credits available to claim from active grants".to_string());
    }

    // Update account credit balance
    let mut account = account.clone();
    let new_credit_balance = account.get_credit_balance() + total_claimed;
    account.token_info.credit_balance = (new_credit_balance as i64) as u64;
    account.updated_at = Some(current_time);
    upsert_account(account)?;

    // Record credit activity
    let activity = CreditActivity {
        timestamp: current_time,
        principal_id: principal_id.to_string(),
        amount: total_claimed,
        activity_type: CreditActivityType::Earn,
        status: TransferStatus::Completed,
        metadata: Some("MCP grant credit claim for all active grants".to_string()),
    };
    record_credit_activity(activity)?;

    Ok(total_claimed)
}

pub fn get_mcp_grant(recipient: &str, mcp_name: &str) -> Option<NewMcpGrant> {
    NEWMCP_GRANTS.with(|grants| {
        let key = NewMcpGrantKey {
            recipient: recipient.to_string(),
            mcp_name: mcp_name.to_string(),
        };
        grants.borrow().get(&key).map(|grant| grant.clone())
    })
}

pub fn get_all_mcp_grants() -> Vec<NewMcpGrant> {
    NEWMCP_GRANTS.with(|grants| {
        grants.borrow()
            .iter()
            .map(|(_, grant)| grant.clone())
            .collect()
    })
}

pub fn get_mcp_grants_paginated(offset: u64, limit: usize) -> Vec<NewMcpGrant> {
    NEWMCP_GRANTS.with(|grants| {
        grants.borrow()
            .iter()
            .skip(offset as usize)
            .take(limit)
            .map(|(_, grant)| grant.clone())
            .collect()
    })
}

pub fn get_mcp_grants_by_recipient(recipient: &str) -> Vec<NewMcpGrant> {
    NEWMCP_GRANTS.with(|grants| {
        grants.borrow()
            .iter()
            .filter(|(_, grant)| grant.recipient == recipient)
            .map(|(_, grant)| grant.clone())
            .collect()
    })
}

pub fn get_mcp_grants_by_mcp(mcp_name: &str) -> Vec<NewMcpGrant> {
    NEWMCP_GRANTS.with(|grants| {
        grants.borrow()
            .iter()
            .filter(|(_, grant)| grant.mcp_name == mcp_name)
            .map(|(_, grant)| grant.clone())
            .collect()
    })
}

pub fn get_mcp_grants_by_status(status: &TokenGrantStatus) -> Vec<NewMcpGrant> {
    NEWMCP_GRANTS.with(|grants| {
        grants.borrow()
            .iter()
            .filter(|(_, grant)| grant.status == *status)
            .map(|(_, grant)| grant.clone())
            .collect()
    })
}

pub fn get_mcp_grants_count() -> u64 {
    NEWMCP_GRANTS.with(|grants| {
        grants.borrow().len() as u64
    })
}

pub fn claim_mcp_grant_with_mcpname(principal_id: &str, mcp_name: &str) -> Result<u64, String> {
    // First check if the account exists
    let account = get_account(principal_id.to_string())
        .ok_or_else(|| "Account not found".to_string())?;

    // Get the specific MCP grant
    let grant = get_mcp_grant(principal_id, mcp_name)
        .ok_or_else(|| format!("No MCP grant found for account {} and MCP {}", principal_id, mcp_name))?;

    if grant.status != TokenGrantStatus::Active {
        return Err("Grant is not active".to_string());
    }

    let remaining_amount = grant.amount - grant.claimed_amount;
    if remaining_amount == 0 {
        return Err("No credits available to claim from this grant".to_string());
    }

    let current_time = time();

    // Update grant status to completed
    let mut updated_grant = grant.clone();
    updated_grant.claimed_amount += remaining_amount;
    updated_grant.status = TokenGrantStatus::Completed;
    create_mcp_grant(updated_grant)?;

    // Update account credit balance
    let mut account = account.clone();
    let new_credit_balance = account.get_credit_balance() + remaining_amount;
    account.token_info.credit_balance = (new_credit_balance as i64) as u64;
    account.updated_at = Some(current_time);
    upsert_account(account)?;

    // Record credit activity
    let activity = CreditActivity {
        timestamp: current_time,
        principal_id: principal_id.to_string(),
        amount: remaining_amount,
        activity_type: CreditActivityType::Earn,
        status: TransferStatus::Completed,
        metadata: Some(format!("MCP grant credit claim for MCP: {}", mcp_name)),
    };
    record_credit_activity(activity)?;

    Ok(remaining_amount)
}

// Add ICRC1 types with different names to avoid conflicts
#[derive(CandidType, Clone, Debug)]
pub struct ICRC1Account {
    pub owner: Principal,
    pub subaccount: Option<Vec<u8>>,
}

#[derive(CandidType, Clone, Debug)]
pub struct ICRC1TransferArgs {
    pub from_subaccount: Option<Vec<u8>>,
    pub to: ICRC1Account,
    pub amount: candid::Nat,
    pub fee: Option<candid::Nat>,
    pub memo: Option<Vec<u8>>,
    pub created_at_time: Option<u64>,
}

#[derive(CandidType, Clone, Debug, Deserialize)]
pub enum ICRC1TransferError {
    BadFee { expected_fee: candid::Nat },
    BadBurn { min_burn_amount: candid::Nat },
    InsufficientFunds { balance: candid::Nat },
    TooOld,
    CreatedInFuture { ledger_time: u64 },
    Duplicate { duplicate_of: candid::Nat },
    TemporarilyUnavailable,
    GenericError { error_code: candid::Nat, message: String },
}

#[derive(CandidType, Clone, Debug, Deserialize)]
pub enum ICRC1TransferResult {
    Ok(candid::Nat),
    Err(ICRC1TransferError),
}

/// Get how many Credits 1 ICP can exchange for currently
pub fn get_credits_per_icp() -> u64 {
    CREDIT_CONVERT_CONTRACT.with(|store| {
        let store = store.borrow();
        let contract = store.get(&CREDIT_CONTRACT_KEY.to_string())
            .unwrap_or(CreditConvertContract {
                price_credits: DEFAULT_CREDIT_USD_PRICE,
                price_icp: DEFAULT_ICP_USD_PRICE,
            });
        (contract.price_icp / contract.price_credits) as u64
    })
}

/// Only admin can update ICP/USD price
pub fn update_icp_usd_price(caller: Principal, new_price: f64) -> Result<(), String> {
    if caller.to_text() != ADMIN_PRINCIPAL {
        return Err("No permission: only admin can operate".to_string());
    }
    CREDIT_CONVERT_CONTRACT.with(|store| {
        let mut store = store.borrow_mut();
        let mut contract = store.get(&CREDIT_CONTRACT_KEY.to_string())
            .unwrap_or(CreditConvertContract {
                price_credits: DEFAULT_CREDIT_USD_PRICE,
                price_icp: DEFAULT_ICP_USD_PRICE,
            });
        contract.price_icp = new_price;
        store.insert(CREDIT_CONTRACT_KEY.to_string(), contract);
        Ok(())
    })
}

/// Simulate recharge, return how many Credits can be obtained
pub fn simulate_credit_from_icp(icp_amount: f64) -> u64 {
    CREDIT_CONVERT_CONTRACT.with(|store| {
        let store = store.borrow();
        let contract = store.get(&CREDIT_CONTRACT_KEY.to_string())
            .unwrap_or(CreditConvertContract {
                price_credits: DEFAULT_CREDIT_USD_PRICE,
                price_icp: DEFAULT_ICP_USD_PRICE,
            });
        ((icp_amount * contract.price_icp) / contract.price_credits) as u64
    })
}

/// Actual recharge, write recharge record and update user balance
pub fn recharge_and_convert_credits(caller: Principal, icp_amount: f64) -> u64 {
    let credits = simulate_credit_from_icp(icp_amount);
    let now = ic_cdk::api::time();
    // Write recharge record
    RECHARGE_RECORDS.with(|records| {
        let mut records = records.borrow_mut();
        let id = records.len() as u64;
        let record = RechargeRecord {
            user: caller,
            icp_amount,
            credits_obtained: credits,
            timestamp: now,
        };
        records.insert(id, record);
    });
    // Update user balance
    let principal_id = caller.to_text();
    let mut account = get_account(principal_id.clone())
        .unwrap_or(AccountInfo::new(principal_id.clone()));
    let new_credit_balance = account.get_credit_balance() + credits;
    account.token_info.credit_balance = (new_credit_balance as i64) as u64;
    account.updated_at = Some(now);
    upsert_account(account).ok();
    credits
}

/// Query user Credit balance
pub fn get_user_credit_balance(principal: Principal) -> u64 {
    let principal_id = principal.to_text();
    get_account(principal_id)
        .map(|acc| acc.get_credit_balance())
        .unwrap_or(0)
}

/// Paginated query of recharge records
pub fn get_recharge_history(principal: Principal, offset: u64, limit: u64) -> Vec<RechargeRecord> {
    RECHARGE_RECORDS.with(|records| {
        let records = records.borrow();
        records.iter()
            .filter(|(_, rec)| rec.user == principal)
            .skip(offset as usize)
            .take(limit as usize)
            .map(|(_, rec)| rec.clone())
            .collect()
    })
}

// ========== ICP Recharge Principal-Account Mapping Table CRUD ==========

/// Add principal-account mapping (only one item allowed)
pub fn add_recharge_principal_account(item: RechargePrincipalAccount) -> Result<(), String> {
    RECHARGE_PRINCIPAL_ACCOUNTS.with(|vec| {
        let vec = vec.borrow_mut();
        // clear all existing items
        while vec.len() > 0 {
            vec.pop();
        }
        // Add the new item
        let _ = vec.push(&item);
        Ok(())
    })
}

/// Get principal-account mapping (returns the single item)
pub fn get_recharge_principal_account() -> Option<RechargePrincipalAccount> {
    RECHARGE_PRINCIPAL_ACCOUNTS.with(|vec| {
        let vec = vec.borrow();
        if vec.len() > 0 {
            Some(vec.get(0).unwrap().clone())
        } else {
            None
        }
    })
}

/// Update principal-account mapping (updates the single item)
pub fn update_recharge_principal_account(item: RechargePrincipalAccount) -> Result<(), String> {
    RECHARGE_PRINCIPAL_ACCOUNTS.with(|vec| {
        let vec = vec.borrow_mut();
        vec.set(0, &item);
        Ok(())
    })
}

/// Delete principal-account mapping (removes the single item)
pub fn delete_recharge_principal_account() -> Result<(), String> {
    RECHARGE_PRINCIPAL_ACCOUNTS.with(|vec| {
        let vec = vec.borrow_mut();
        if vec.len() > 0 {
            while vec.len() > 0 {
                vec.pop();
            }
            Ok(())
        } else {
            Err("No principal account mapping exists to delete".to_string())
        }
    })
}

/// Get principal-account mapping list (returns the single item if exists)
pub fn list_recharge_principal_accounts() -> Vec<RechargePrincipalAccount> {
    RECHARGE_PRINCIPAL_ACCOUNTS.with(|vec| {
        let vec = vec.borrow();
        if vec.len() > 0 {
            vec![vec.get(0).unwrap().clone()]
        } else {
            vec![]
        }
    })
} 