# Add backend logic for customs ordering AI Service 
- for datatype
## 1 Add datatype .rs file with name: ai_subscription_types.rs
add as stable_mem implements, you can refer to other data types codes.
all data types need to be managered by aio-base-backend/src/stable_mem_storage.rs
service_type{
    svr_id,
    name,
    price_level, //M:month; Y:year
    price,//suit for usdt data type
}

subscription_record{
    principal_id, 
    pay_walletid,
    svr_id,
    pay_date,//yyyyMMdd
    status //0-normal, -1 - resolved

}

## 2 Add ai_sub_service.rs in aio-base-backend/src
Add crud related function for #1 data types
You also can add some important functons which could be considered really useful for business scene of AI service subscrition

## 3 Export #2 functions to aio-base-backend/src/lib.rs

## 4 Add new config scripts in aio-base-backend.did