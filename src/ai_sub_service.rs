// CRUD and business logic for AI subscription (service types and subscription records).
// See prompts_ai_subscribe.md and ai_subscription_types.rs.

use crate::ai_subscription_types::{
    PriceLevel, PrincipalSubscriptionKey, ServiceType, SubscriptionRecord, SubscriptionStatus,
};
use crate::stable_mem_storage::{AI_SERVICES, SUBSCRIPTION_PRINCIPAL_INDEX, SUBSCRIPTION_RECORDS};

// ---------- ServiceType CRUD ----------

pub fn create_service(service: ServiceType) -> Result<(), String> {
    let svr_id = service.svr_id.clone();
    if svr_id.is_empty() {
        return Err("svr_id is required".to_string());
    }
    AI_SERVICES.with(|m| {
        let mut map = m.borrow_mut();
        if map.contains_key(&svr_id) {
            return Err(format!("Service {} already exists", svr_id));
        }
        map.insert(svr_id, service);
        Ok(())
    })
}

pub fn get_service(svr_id: &str) -> Option<ServiceType> {
    AI_SERVICES.with(|m| m.borrow().get(&svr_id.to_string()))
}

pub fn update_service(svr_id: &str, service: ServiceType) -> Result<(), String> {
    if service.svr_id != svr_id {
        return Err("svr_id in body must match path".to_string());
    }
    AI_SERVICES.with(|m| {
        let mut map = m.borrow_mut();
        if !map.contains_key(&svr_id.to_string()) {
            return Err(format!("Service {} not found", svr_id));
        }
        map.insert(svr_id.to_string(), service);
        Ok(())
    })
}

pub fn delete_service(svr_id: &str) -> Result<(), String> {
    AI_SERVICES.with(|m| {
        let mut map = m.borrow_mut();
        if map.remove(&svr_id.to_string()).is_some() {
            Ok(())
        } else {
            Err(format!("Service {} not found", svr_id))
        }
    })
}

pub fn list_services() -> Vec<ServiceType> {
    AI_SERVICES.with(|m| m.borrow().iter().map(|(_, v)| v.clone()).collect())
}

pub fn list_services_paginated(offset: u64, limit: usize) -> Vec<ServiceType> {
    AI_SERVICES.with(|m| {
        m.borrow()
            .iter()
            .skip(offset as usize)
            .take(limit)
            .map(|(_, v)| v.clone())
            .collect()
    })
}

pub fn service_count() -> u64 {
    AI_SERVICES.with(|m| m.borrow().len()) as u64
}

// ---------- SubscriptionRecord CRUD ----------

pub fn create_subscription_record(record: SubscriptionRecord) -> Result<u64, String> {
    if record.principal_id.is_empty() || record.svr_id.is_empty() || record.pay_date.is_empty() {
        return Err("principal_id, svr_id and pay_date are required".to_string());
    }
    SUBSCRIPTION_RECORDS.with(|vec_storage| {
        SUBSCRIPTION_PRINCIPAL_INDEX.with(|idx_storage| {
            let mut vec_mut = vec_storage.borrow_mut();
            let index = vec_mut.len();
            vec_mut.push(&record);
            let key = PrincipalSubscriptionKey {
                principal_id: record.principal_id,
                index,
            };
            idx_storage.borrow_mut().insert(key, ());
            Ok(index as u64)
        })
    })
}

pub fn get_subscription_record(index: u64) -> Option<SubscriptionRecord> {
    SUBSCRIPTION_RECORDS.with(|m| m.borrow().get(index))
}

pub fn update_subscription_status(index: u64, status: SubscriptionStatus) -> Result<(), String> {
    let mut rec = SUBSCRIPTION_RECORDS
        .with(|m| m.borrow().get(index))
        .ok_or_else(|| "Record not found".to_string())?;
    rec.status = status;
    SUBSCRIPTION_RECORDS.with(|m| {
        m.borrow_mut().set(index, &rec);
    });
    Ok(())
}

/// Resolve (cancel/expire) a subscription record by index
pub fn resolve_subscription(index: u64) -> Result<(), String> {
    update_subscription_status(index, SubscriptionStatus::Resolved)
}

pub fn list_subscriptions_by_principal(principal_id: &str) -> Vec<SubscriptionRecord> {
    let start_key = PrincipalSubscriptionKey {
        principal_id: principal_id.to_string(),
        index: 0,
    };
    let end_key = PrincipalSubscriptionKey {
        principal_id: principal_id.to_string(),
        index: u64::MAX,
    };
    let indices: Vec<u64> = SUBSCRIPTION_PRINCIPAL_INDEX.with(|idx| {
        idx.borrow()
            .range(start_key..=end_key)
            .map(|(k, _)| k.index)
            .collect()
    });
    SUBSCRIPTION_RECORDS.with(|vec_storage| {
        let vec_ref = vec_storage.borrow();
        indices
            .into_iter()
            .filter_map(|i| vec_ref.get(i))
            .collect()
    })
}

pub fn list_subscriptions_by_principal_paginated(
    principal_id: &str,
    offset: u64,
    limit: usize,
) -> Vec<SubscriptionRecord> {
    let all = list_subscriptions_by_principal(principal_id);
    all.into_iter()
        .skip(offset as usize)
        .take(limit)
        .collect()
}

pub fn subscription_record_count() -> u64 {
    SUBSCRIPTION_RECORDS.with(|m| m.borrow().len()) as u64
}

// ---------- Business helpers ----------

/// Returns true if principal has at least one active (Normal) subscription for the given service
pub fn is_subscribed(principal_id: &str, svr_id: &str) -> bool {
    list_subscriptions_by_principal(principal_id)
        .into_iter()
        .any(|r| r.svr_id == svr_id && r.status == SubscriptionStatus::Normal)
}

/// All active (Normal) subscription records for a principal
pub fn get_active_subscriptions(principal_id: &str) -> Vec<SubscriptionRecord> {
    list_subscriptions_by_principal(principal_id)
        .into_iter()
        .filter(|r| r.status == SubscriptionStatus::Normal)
        .collect()
}
