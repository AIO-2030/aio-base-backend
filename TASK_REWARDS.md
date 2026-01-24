# Cursor Implementation Prompt — ICP Backend Canister (Rust) + Merkle Snapshot + Task Incentives + Payment Ledger

你是项目的 Rust/ICP 工程师。请在当前 repo 中为 backend canister 增加一套“用户任务激励 + 支付流水 + Merkle Distributor 快照 + Claim Ticket API”的完整实现。必须遵守以下约束：

## 0. 强约束与交付物

### 必须新增文件
- 新建：`src/aio-base-backend/src/task_rewards.rs`（或与项目结构匹配的 canister crate 下同级新文件）
- 该文件包含：数据结构、稳定存储接口封装、公开 Candid 方法、merkle 快照生成与 proof 查询逻辑。

### 必须复用统一稳定存储
- 必须通过项目已存在的 `stable_mem_storage.rs`（统一维护）实现静态存储。
- 不允许另起一套 stable memory 体系；必须在 `stable_mem_storage.rs` 里注册/扩展需要的 StableBTreeMap / StableCell / StableVec 等结构。
- 需要说明每个存储结构选择原因（按需最小化）。

### 必须新增脚本初始化
- 修改 `build-aichat.sh`：
  - 在 `dfx deploy` 完成后，自动调用 backend canister 的初始化方法写入“任务激励模型合约数据”。
  - 初始化任务要素：`taskid: String, rewards: nat64, valid: bool`
  - 固定三项任务：
    1) register_device
    2) ai_subscription
    3) voice_clone

### 必须新增 API（Candid 方法）
需提供以下 Candid 对外接口（名称可略调整但语义一致）：
1) `init_task_contract(tasks: Vec<TaskContractItem>) -> Result<()>`  (admin)
2) `get_task_contract() -> Vec<TaskContractItem>`
3) `get_or_init_user_tasks(wallet: String) -> UserTaskState`  (user)  
   - “用户登录，检索 task details，如果没有则初始化”
4) `record_payment(wallet: String, amount_paid: nat64, tx_ref: String, ts: nat64) -> Result<()>` (user)
   - 写入支付流水；并根据业务逻辑更新 task 状态（至少 AI 订阅任务完成/可奖励）
5) `complete_task(wallet: String, taskid: String, evidence: Option<String>, ts: nat64) -> Result<()>` (user)
   - 用于注册设备/语音复刻等，写入完成记录并更新状态
6) `build_epoch_snapshot(epoch: nat64) -> Result<MerkleSnapshotMeta>` (admin or scheduled)
   - 生成本 epoch 的 merkle 快照（root），并冻结 claimable 列表
7) `get_claim_ticket(wallet: String) -> Result<ClaimTicket>` (user)
   - 返回 `{epoch, index, amount, proof, root}` 给前端，前端提交 Solana 主链 claim 合约
   - 同时：在后端把该 wallet 的当前 epoch 的 claim 状态标记为 “ticket_issued / claimed_pending”，避免重复发票（但链上仍以 ClaimStatus PDA 为最终防线）
8) `mark_claim_result(wallet: String, epoch: nat64, status: ClaimResultStatus, tx_sig: Option<String>) -> Result<()>` (user or server)
   - 让前端在链上 claim 成功后回写以便后端对账

> 注意：本项目的“用户身份”以 `wallet: String` 为主键（Solana pubkey base58）,因用户每次的绑定钱包可能不同，因此在流水记录中只需要在真实产生资金动作：claim,pay的时候维护当前的princalid和wallet address关系，但是这个关系仅为当次有效，不需要作为规则

---

## 1. 数据模型（必须实现）

### 1.1 任务合约模型（全局）
```rust
pub struct TaskContractItem {
  pub taskid: String,
  pub rewards: u64,     // nat64
  pub valid: bool,
}

1.2 用户任务状态
	•	用户每个 task 需要状态机字段：
	•	status: NotStarted | InProgress | Completed | RewardPrepared | Claimed
	•	completed_at: Option<u64>
	•	reward_amount: u64（从 TaskContractItem 写入，未来可扩展动态）
	•	evidence: Option<String>（可选）
	•	用户主结构：

pub struct UserTaskState {
  pub wallet: String,
  pub tasks: Vec<UserTaskDetail>,
  pub updated_at: u64,
}

1.3 支付流水（AI 订阅支付）
	•	记录支付事件（写 stable）：

pub struct PaymentRecord {
  pub wallet: String,
  pub amount_paid: u64,
  pub tx_ref: String,   // 可是 ICP 内部订单号/第三方支付号/链上 tx
  pub ts: u64,
  pub payfor:String, //如 AI_ORDER  VOICE_DUP
}

	•	支付后业务逻辑：
	•	至少将 ai_subscription task 标记为 Completed/RewardPrepared
	•	同时写入“待 claim 明细”来源数据（见 claimable entries）

1.4 Claimable entries（用于生成 Merkle）
	•	在每个 epoch 需要冻结一份“分配表 entries”，每条 entry 对应一个 leaf：
	•   epoch触发机制：定时结算（24小时一次），但是注意，如果当前没有待结算流水，不需要触发


pub struct ClaimEntry {
  pub epoch: u64,
  pub index: u32,
  pub wallet: String,   // solana pubkey base58
  pub amount: u64,      // PMUG 最小单位
}

1.5 Merkle 快照元数据

pub struct MerkleSnapshotMeta {
  pub epoch: u64,
  pub root: [u8;32],
  pub leaves_count: u32,
  pub created_at: u64,
}

1.6 Claim Ticket（前端用）

pub struct ClaimTicket {
  pub epoch: u64,
  pub index: u32,
  pub wallet: String,
  pub amount: u64,
  pub proof: Vec<[u8;32]>,
  pub root: [u8;32],
}


⸻

2. Merkle Tree 规范（必须写死并在注释中强调一致性）

为了与 Solana Anchor 合约验证一致，采用最稳的规范：

Leaf 编码（强制）

leaf_hash = SHA256(
epoch(u64 little-endian 8 bytes)
|| index(u32 little-endian 4 bytes)
|| wallet_pubkey(32 bytes, base58 decode)
|| amount(u64 little-endian 8 bytes)
)

Node 合并（强制推荐“排序哈希”）

parent = SHA256( min(left,right) || max(left,right) )
	•	这样 proof 不需要记录方向位（节省存储/减少前后端对齐风险）
	•	Solana 合约也要用同样规则（后续由前端/合约侧对齐）

如果 repo 已经在前端或其他模块定义了 hash 规则，必须与其保持一致；否则以此规范为准并全局使用。

⸻

3. Merkle Tree 最优方案（必须节省 stable memory）

目标：在 canister 里支持 get_claim_ticket(wallet) 快速生成 proof，同时节省空间。

推荐方案（必须实现）：存储 tree layers（中间层）而不是存每个用户 proof

对每个 epoch：
	•	保存 layers: Vec<Vec<[u8;32]>>
	•	layer[0] = leaf_hashes（长度 N）
	•	layer[1] = parents（长度 ceil(N/2)）
	•	…
	•	最后一层长度 1，就是 root
	•	保存 wallet -> (epoch,index,amount) 映射（或至少 index）
这样：
	•	proof 生成：对给定 index，逐层取 sibling（O(logN)）
	•	存储量：总节点数 ~ 2N（比“为每人存 proof”小得多，且可控）
	•	不保存明文 leaf 列表也可以（但建议同时保存 ClaimEntry 的 amount + wallet->index 以便对账）

存储结构建议（要落到 stable_mem_storage.rs）
	•	TASK_CONTRACT: StableBTreeMap<String, TaskContractItem>
	•	USER_TASKS: StableBTreeMap<String, UserTaskState> (key=wallet)
	•	PAYMENTS: StableBTreeMap<(String,u64,String), PaymentRecord> 或 append-only StableVec + index
	•	EPOCH_META: StableBTreeMap<u64, MerkleSnapshotMeta>
	•	EPOCH_WALLET_INDEX: StableBTreeMap<(u64,String), (u32,u64)> // epoch+wallet -> (index,amount)
	•	EPOCH_LAYERS: StableVec + 二级索引：
	•	由于 StableBTreeMap 不适合巨大 Vec<Vec>，请使用：
	•	StableVec<[u8;32]> 作为扁平存储
	•	维护 EPOCH_LAYER_OFFSETS: StableBTreeMap<(u64, layer_id), (start: u64, len: u32)>
	•	这样可以在 stable memory 中连续存储 hash，避免嵌套 Vec 占用堆内存。

必须给出清晰实现：
	•	build_epoch_snapshot 时：
	•	计算 leaf_hashes
	•	逐层构建 parents
	•	将每层扁平写入 StableVec，记录 offsets
	•	root 写入 meta 与 distributor root 对齐

⸻

4. 业务流程（必须实现的状态转移）

4.1 用户登录初始化
	•	get_or_init_user_tasks(wallet)：
	•	如果 USER_TASKS 没有该 wallet：
	•	从 TASK_CONTRACT 读取全部 valid tasks，初始化 tasks 列表为 NotStarted
	•	写入 USER_TASKS
	•	返回 UserTaskState

4.2 完成任务
	•	complete_task(wallet, taskid, evidence, ts)：
	•	校验 task 在合约里 valid
	•	更新 UserTaskDetail：
	•	status -> Completed
	•	completed_at=ts
	•	reward_amount=contract.rewards
	•	同时把该用户加入“待 claim 汇总”：可以只更新 user tasks，真正 entries 在 build_epoch_snapshot 冻结时生成

4.3 记录支付并触发 AI 订阅任务完成
	•	record_payment：
	•	追加 PaymentRecord
	•	更新 task ai_subscription 完成（同 complete_task 逻辑）
	•	可根据支付金额决定奖励是否有效（MVP 不做复杂规则，直接完成即可）

4.4 周期性结算生成 epoch 快照（Merkle）
	•	build_epoch_snapshot(epoch)：
	•	读取 USER_TASKS，收集所有 status==Completed 且未进入 RewardPrepared/Claimed 的奖励
	•	生成 ClaimEntry 列表（对每个 wallet 汇总 amount 或按任务叠加）
	•	对 entries 按 wallet（base58 字符串）排序，生成 index
	•	计算 leaf_hashes -> layers -> root
	•	写入：
	•	EPOCH_META[epoch]
	•	EPOCH_WALLET_INDEX[(epoch,wallet)] = (index, amount)
	•	EPOCH_LAYERS offsets + data
	•	同时把用户对应任务状态转为 RewardPrepared，并记录 epoch（可在 UserTaskDetail 中新增 prepared_epoch 字段）

4.5 用户发起 claim 获取 ticket
	•	get_claim_ticket(wallet)：
	•	找到该 wallet 最近可 claim 的 epoch（在 EPOCH_WALLET_INDEX 中查）
	•	取出 (index, amount) + root + layers offsets
	•	计算 proof（逐层取 sibling）
	•	返回 ClaimTicket
	•	同时把用户任务状态标记 “ClaimedPending / TicketIssued”，避免无限刷新拿票据

4.6 链上结果回写
	•	epoch外部运营脚本提交到solana链上后回写canister
	•	mark_claim_result(wallet, epoch, status, tx_sig)：
	•	成功则把相关任务标记 Claimed
	•	失败则回滚到 RewardPrepared 以允许重试（但要避免重复 epoch 票据滥发）

⸻

5. 安全与一致性要求（必须加注释与校验）
	•	base58 decode wallet 时校验长度=32 bytes，否则拒绝
	•	epoch 不允许覆盖已存在 EPOCH_META（除非显式 admin reset）
	•	build_epoch_snapshot 必须 deterministic：排序规则固定（按 wallet 字符串升序）
	•	claim_ticket 返回的 root 必须等于 EPOCH_META.root
	•	用户重复 get_claim_ticket 要么返回同一票据，要么拒绝（由 TicketIssued 状态决定）

⸻

6. build-aichat.sh 修改要求（必须实现）

在 dfx deploy 后追加：
	•	调用 backend 的 init_task_contract（需要 admin identity）
	•	传入 3 个任务：
	•	(“register_device”, rewards=?, valid=true)
	•	(“ai_subscription”, rewards=?, valid=true)
	•	(“voice_clone”, rewards=?, valid=true)

奖励数值先用占位（例如 100_000_000）并在注释注明单位为 PMUG 最小单位。

⸻

7. 必须生成一份前端接入提示词（MD 文件）

你必须基于你对当前前端工程的理解（自行在 repo 内搜索 wallet 连接、claim 页面、API client 代码），生成一个新文件：
	•	src/alaya-chat-nexus-frontend/frontend-claim-integration-prompt.md

该文件内容必须是一段“给 Cursor 修改前端用的提示词”，包含：
	1.	前端需要新增/调整的 API 调用点（get_or_init_user_tasks, record_payment, complete_task, get_claim_ticket, mark_claim_result）
	2.	claim 页面时序：
	•	连接 Phantom
	•	wallet address
	•	调 canister get_claim_ticket
	•	组装并调用 Solana claim 合约（Anchor / web3.js）
	•	成功后回写 mark_claim_result
	3.	UI 状态建议：
	•	loading / ticket-ready / tx-sent / success / failed
	4.	数据结构对齐（ClaimTicket 字段、proof 的 bytes 格式与 base58/hex 编码建议）
	5.	PC 端只走 Phantom 插件（不走 WalletConnect），移动端另行处理（此文档只针对 PC）
	6.	root 未上链 → 不展示 claim 按钮，root 上链 → 才允许用户 get_claim_ticket / claim

⸻

8. 输出要求
	•	提供完整 Rust 代码（task_rewards.rs + 必要的 stable_mem_storage.rs 扩展 + candid 导出更新）
	•	提供 build-aichat.sh patch
	•	提供 docs/frontend-claim-integration-prompt.md
	•	所有 public 方法要有清晰注释，尤其是 Merkle 规范与 proof 生成逻辑
	•	写最少必要依赖：hash 用 sha2，base58 decode 用 bs58（如项目已有依赖，复用）
	•	代码要可编译通过，并适配现有 canister 模块结构（必要时自行定位 lib.rs/mod.rs 注册）
	•	生成一份epoch提交上链的运营脚本， src/aio-base-backend/scripts


