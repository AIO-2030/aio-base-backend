# Solana 合约实现检查清单

## 快速开始

### ✅ 前置条件

- [ ] 已安装 Anchor CLI
- [ ] 已安装 Node.js 和 npm
- [ ] 已安装 Solana CLI
- [ ] 已配置 Solana 钱包

### ✅ 项目初始化

```bash
# 1. 创建项目
anchor init merkle-distributor
cd merkle-distributor

# 2. 安装依赖
npm install @coral-xyz/anchor @solana/web3.js @solana/spl-token
npm install --save-dev @types/node typescript ts-mocha mocha chai

# 3. 查看详细实现指南
# 参考: SOLANA_CONTRACT_IMPLEMENTATION_PROMPT.md
```

## 实现步骤

### Phase 1: 基础结构

- [ ] 创建 `lib.rs` 文件
- [ ] 定义 `Distributor` 账户结构
- [ ] 定义 `EpochData` 账户结构
- [ ] 定义 `ClaimStatus` 账户结构
- [ ] 实现 `initialize` 方法
- [ ] 实现错误码枚举

### Phase 2: Merkle Tree 实现

- [ ] 实现 `compute_leaf_hash` 函数
  - [ ] 使用 SHA256
  - [ ] epoch (u64 le)
  - [ ] index (u32 le)
  - [ ] wallet_pubkey (32 bytes)
  - [ ] amount (u64 le)
- [ ] 实现 `verify_merkle_proof` 函数
  - [ ] 使用排序哈希：`SHA256(min(left, right) || max(left, right))`
  - [ ] 验证最终 hash 等于 root
- [ ] 与后端 `task_rewards.rs` 验证一致性

### Phase 3: 核心功能

- [ ] 实现 `update_epoch_root` 方法
  - [ ] 管理员权限检查
  - [ ] Epoch 防重复
  - [ ] 存储 root 和 leaves_count
- [ ] 实现 `claim` 方法
  - [ ] Epoch 验证
  - [ ] 防重复 claim（ClaimStatus PDA）
  - [ ] Merkle proof 验证
  - [ ] 代币转账
- [ ] 实现 `get_epoch_info` 查询方法

### Phase 4: 测试

- [ ] 编写初始化测试
- [ ] 编写更新 epoch root 测试
- [ ] 编写 claim 测试
  - [ ] 使用真实的 Merkle proof
  - [ ] 验证代币转账
  - [ ] 验证防重复 claim
- [ ] 运行所有测试：`anchor test`

### Phase 5: 部署

- [ ] 构建合约：`anchor build`
- [ ] 获取 Program ID：`anchor keys list`
- [ ] 更新 `lib.rs` 中的 `declare_id!`
- [ ] 更新 `Anchor.toml` 配置
- [ ] 重新构建：`anchor build`
- [ ] 部署到 Devnet：`anchor deploy --provider.cluster devnet`
- [ ] 验证部署：检查 Solana Explorer

### Phase 6: 集成

- [ ] 更新 `epoch_submit.sh` 中的配置
  - [ ] `DISTRIBUTOR_PROGRAM_ID`
  - [ ] `DISTRIBUTOR_ADDRESS` (Distributor PDA)
- [ ] 测试端到端流程
  1. 后端生成 epoch 快照
  2. 脚本提交 root 到 Solana
  3. 用户获取 claim ticket
  4. 前端调用合约 claim
  5. 验证代币到账

## 关键验证点

### Merkle Tree 兼容性

- [ ] Leaf hash 计算与后端一致
- [ ] Parent hash 使用排序哈希
- [ ] 字节序为 little-endian
- [ ] Wallet pubkey 为 32 字节原始格式

### 安全性

- [ ] 所有管理员操作有权限检查
- [ ] Epoch 不能重复更新
- [ ] Claim 不能重复（ClaimStatus PDA）
- [ ] Merkle proof 验证正确
- [ ] 代币余额检查

### 功能完整性

- [ ] 初始化功能正常
- [ ] 更新 epoch root 功能正常
- [ ] Claim 功能正常
- [ ] 查询功能正常
- [ ] 错误处理完善

## 部署前检查

- [ ] 所有测试通过
- [ ] 代码审查完成
- [ ] 与后端兼容性验证
- [ ] 安全审计（如需要）
- [ ] 文档完整
- [ ] 配置正确（Program ID、PDA 地址等）

## 主网部署

- [ ] 在 Devnet 完整测试
- [ ] 准备主网部署脚本
- [ ] 确认代币 mint 地址
- [ ] 确认管理员账户
- [ ] 准备足够的 SOL 支付费用
- [ ] 备份密钥和配置
- [ ] 执行部署：`anchor deploy --provider.cluster mainnet-beta`
- [ ] 验证部署成功
- [ ] 更新生产环境配置

## 故障排除

### 常见问题

- [ ] Program ID 不匹配 → 检查 `declare_id!` 和 `Anchor.toml`
- [ ] PDA 计算错误 → 检查 seeds 和 bump
- [ ] Merkle proof 验证失败 → 检查 hash 计算是否与后端一致
- [ ] 代币转账失败 → 检查 vault 余额和权限
- [ ] 测试失败 → 检查账户初始化和余额

### 调试技巧

```bash
# 查看程序账户
solana program show <PROGRAM_ID> --url devnet

# 查看账户数据
solana account <ACCOUNT_ADDRESS> --url devnet

# 查看交易详情
solana confirm <TX_SIGNATURE> --url devnet

# Anchor 日志
anchor test --skip-local-validator
```

## 相关文件

- 📄 `SOLANA_CONTRACT_IMPLEMENTATION_PROMPT.md` - 完整实现指南
- 📄 `SOLANA_CONFIG_EXPLAINED.md` - 配置参数说明
- 📄 `epoch_submit.sh` - Epoch 提交脚本
- 📄 `../src/task_rewards.rs` - 后端实现参考

## 完成标志

当以下所有项都完成时，合约实现就完成了：

- ✅ 合约代码实现完成
- ✅ 所有测试通过
- ✅ 部署到 Devnet 成功
- ✅ 与后端集成测试通过
- ✅ 文档完整
- ✅ 准备部署到主网

---

**提示：** 按照 `SOLANA_CONTRACT_IMPLEMENTATION_PROMPT.md` 中的详细说明逐步实现。
