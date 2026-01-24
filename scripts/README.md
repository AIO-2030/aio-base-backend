# Epoch 提交上链运营脚本

本目录包含用于将 Merkle root 提交到 Solana 链上的运营脚本。

## 文件说明

- `epoch_submit.sh` - Bash 版本的提交脚本
- `epoch_submit.py` - Python 版本的提交脚本
- `README.md` - 本说明文档

## 使用前准备

### 1. 配置脚本

编辑脚本文件，修改以下配置项：

```bash
# ICP Canister ID
BACKEND_CANISTER_ID="your-backend-canister-id"

# Solana Configuration  
DISTRIBUTOR_PROGRAM_ID="your-distributor-program-id"
DISTRIBUTOR_ADDRESS="your-distributor-address"
```

### 2. 安装依赖

**Bash 脚本依赖：**
- dfx CLI
- solana CLI
- Node.js (用于 Solana 交易提交)
- @solana/web3.js, @project-serum/anchor (Node.js 包)

```bash
# 安装 dfx
sh -c "$(curl -fsSL https://internetcomputer.org/install.sh)"

# 安装 Solana CLI
sh -c "$(curl -sSfL https://release.solana.com/stable/install)"

# 安装 Anchor CLI (可选)
cargo install --git https://github.com/coral-xyz/anchor avm --locked --force

# 安装 Node.js 依赖
npm install @solana/web3.js @project-serum/anchor
```

**Python 脚本依赖：**
```bash
pip install solders solana anchorpy
```

### 3. 配置 Solana 密钥

确保管理员密钥文件存在：
```bash
~/.config/solana/id.json
```

如果没有，生成一个新的：
```bash
solana-keygen new -o ~/.config/solana/id.json
```

## 使用方法

### 定时任务（推荐）

建议设置 cron 定时任务，每24小时自动执行一次：

```bash
# 编辑 crontab
crontab -e

# 添加以下行（每天凌晨2点执行）
0 2 * * * /path/to/epoch_submit.sh $(date +\%s) >> /var/log/epoch_submit.log 2>&1
```

### 手动执行

**Bash 版本：**
```bash
chmod +x epoch_submit.sh
./epoch_submit.sh 1
```

**Python 版本：**
```bash
chmod +x epoch_submit.py
python3 epoch_submit.py 1
```

其中 `1` 是 epoch 编号，应该递增使用。

## 工作流程

脚本执行以下步骤：

1. **在 ICP 后端生成快照**
   - 调用 `build_epoch_snapshot(epoch)` 
   - 收集所有已完成但未准备的任务奖励
   - 生成 Merkle tree
   - 冻结奖励列表

2. **获取 Epoch 元数据**
   - 调用 `get_epoch_meta(epoch)`
   - 获取 Merkle root 和叶子数量

3. **提交 Root 到 Solana**
   - 连接 Solana 网络
   - 调用 Distributor 合约的 `update_epoch_root` 方法
   - 将 root 写入链上

4. **验证**
   - 在 Solana Explorer 上查看交易
   - 用户现在可以通过前端 claim 奖励

## 注意事项

### Epoch 编号管理

- Epoch 应该按顺序递增：1, 2, 3, ...
- 不要跳过或重复使用 epoch 编号
- 建议维护一个文件记录当前 epoch：

```bash
# 读取当前 epoch
CURRENT_EPOCH=$(cat /var/lib/epoch_counter.txt)

# 执行提交
./epoch_submit.sh $CURRENT_EPOCH

# 递增并保存
echo $((CURRENT_EPOCH + 1)) > /var/lib/epoch_counter.txt
```

### 错误处理

脚本会在以下情况退出：
- ICP canister 调用失败
- Epoch 快照已存在（不能重复生成）
- 没有可结算的奖励
- Solana 交易提交失败

检查日志以诊断问题。

### 监控

建议设置监控和告警：
- 监控脚本执行状态
- 监控 Solana 交易确认
- 监控用户 claim 情况

## 实现 Solana 提交逻辑

**重要：脚本中的 Solana 提交部分需要根据你的实际合约实现**

### 示例：使用 Anchor

假设你的 Distributor 合约有以下接口：

```rust
pub fn update_epoch_root(
    ctx: Context<UpdateEpochRoot>,
    epoch: u64,
    root: [u8; 32],
    leaves_count: u32,
) -> Result<()>
```

Node.js 提交代码示例：

```javascript
const tx = await program.methods
  .updateEpochRoot(
    new anchor.BN(epoch),
    Array.from(root),
    leaves_count
  )
  .accounts({
    distributor: distributorAddress,
    admin: adminKeypair.publicKey,
  })
  .signers([adminKeypair])
  .rpc();
```

### 验证提交

提交后，应该验证：
1. 交易成功确认
2. Distributor 账户的 epoch 数据已更新
3. Root 值正确

```bash
# 查询 Distributor 账户
solana account <DISTRIBUTOR_ADDRESS> -u mainnet-beta

# 或使用 Anchor
anchor account merkle_distributor <DISTRIBUTOR_ADDRESS>
```

## 故障排除

### 问题：build_epoch_snapshot 失败

**可能原因：**
- Epoch 已存在
- 没有可结算的奖励
- 权限不足（不是 controller）

**解决方法：**
- 检查 epoch 是否已被使用：`dfx canister call <canister> get_epoch_meta '(1 : nat64)'`
- 检查是否有用户完成了任务
- 确认使用 controller identity 执行

### 问题：Solana 交易失败

**可能原因：**
- 余额不足
- 合约账户状态不正确
- 权限检查失败

**解决方法：**
- 检查钱包余额：`solana balance`
- 查看交易日志：在 Solana Explorer 上查看失败原因
- 检查 admin 权限设置

### 问题：Root 不匹配

**可能原因：**
- Merkle tree 生成逻辑不一致
- 叶子排序方式不同

**解决方法：**
- 确保后端和前端使用相同的 Merkle tree 规范
- 检查哈希算法和字节序

## 安全建议

1. **保护密钥文件**
   ```bash
   chmod 600 ~/.config/solana/id.json
   ```

2. **使用专用的 admin 账户**
   - 不要使用个人钱包
   - 只授予必要的权限

3. **审计日志**
   - 保存所有脚本执行日志
   - 记录所有 Solana 交易签名

4. **多签验证（可选）**
   - 对于主网，考虑使用多签钱包
   - 需要多个管理员批准才能提交 root

## 联系与支持

如有问题，请联系开发团队或查看项目文档。

## 许可证

（根据项目许可证填写）
