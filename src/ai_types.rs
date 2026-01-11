use candid::{CandidType, Decode, Encode};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::{DefaultMemoryImpl, StableBTreeMap};
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cell::RefCell;
use crate::stable_mem_storage::USER_AI_CONFIG;

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UserAiConfig {
    pub principal_id: String,
    pub agent_id: String,
    pub voice_id: String,
}

// Key for user AI config lookup by principal_id
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PrincipalKey {
    pub principal_id: String,
}

impl ic_stable_structures::Storable for PrincipalKey {
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(&self.principal_id).unwrap())
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        let principal_id = Decode!(bytes.as_ref(), String).unwrap();
        Self { principal_id }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 200,
        is_fixed_size: false,
    };
}

impl ic_stable_structures::Storable for UserAiConfig {
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 1000,
        is_fixed_size: false,
    };
}

// Get user AI config by principal_id
pub fn get_user_ai_config(principal_id: String) -> Option<UserAiConfig> {
    USER_AI_CONFIG.with(|config_map| {
        let key = PrincipalKey { principal_id };
        config_map.borrow().get(&key)
    })
}

// Set or update user AI config
pub fn set_user_ai_config(config: UserAiConfig) -> Result<(), String> {
    USER_AI_CONFIG.with(|config_map| {
        let key = PrincipalKey {
            principal_id: config.principal_id.clone(),
        };
        config_map.borrow_mut().insert(key, config);
        Ok(())
    })
}

// Delete user AI config
pub fn delete_user_ai_config(principal_id: String) -> Result<(), String> {
    USER_AI_CONFIG.with(|config_map| {
        let key = PrincipalKey { principal_id };
        if config_map.borrow_mut().remove(&key).is_some() {
            Ok(())
        } else {
            Err("User AI config not found".to_string())
        }
    })
}

// Check if user has AI config
pub fn has_user_ai_config(principal_id: String) -> bool {
    USER_AI_CONFIG.with(|config_map| {
        let key = PrincipalKey { principal_id };
        config_map.borrow().contains_key(&key)
    })
}
