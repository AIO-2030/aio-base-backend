// AI subscription data types for custom ordering AI Service.
// Managed by stable_mem_storage; see prompts_ai_subscribe.md.

use candid::{CandidType, Decode, Encode};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Price level: M = month, Y = year, E = permanent (永久生效)
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PriceLevel {
    M, // month
    Y, // year
    E, // permanent 永久生效
}

/// AI service type: catalog entry for a subscribable AI service
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ServiceType {
    pub svr_id: String,
    pub name: String,
    pub price_level: PriceLevel,
    /// Price in USDT-compatible units (e.g. cents or smallest unit)
    pub price: u64,
}

/// Subscription status: 0 = normal (active), -1 = resolved (cancelled/expired)
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SubscriptionStatus {
    Normal = 0,
    Resolved = -1,
}

impl SubscriptionStatus {
    pub fn from_i8(v: i8) -> Self {
        match v {
            0 => SubscriptionStatus::Normal,
            _ => SubscriptionStatus::Resolved,
        }
    }
}

/// Single subscription/payment record
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SubscriptionRecord {
    pub principal_id: String,
    pub pay_walletid: String,
    pub svr_id: String,
    pub pay_date: String, // yyyyMMdd
    pub status: SubscriptionStatus,
}

// --- Storable impls for stable memory ---

impl Storable for ServiceType {
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }
    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
    const BOUND: Bound = Bound::Bounded {
        max_size: 2048,
        is_fixed_size: false,
    };
}

impl Storable for SubscriptionRecord {
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }
    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
    const BOUND: Bound = Bound::Bounded {
        max_size: 1024,
        is_fixed_size: false,
    };
}

/// Key for indexing subscription records by (principal_id, record_index).
/// Serialized manually so we don't need CandidType for stable memory.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PrincipalSubscriptionKey {
    pub principal_id: String,
    pub index: u64,
}

impl Storable for PrincipalSubscriptionKey {
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(&(&self.principal_id, self.index)).unwrap())
    }
    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        let (principal_id, index) = Decode!(bytes.as_ref(), (String, u64)).unwrap();
        Self { principal_id, index }
    }
    const BOUND: Bound = Bound::Bounded {
        max_size: 256,
        is_fixed_size: false,
    };
}
