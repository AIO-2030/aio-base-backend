# Solana 配置参数说明

本文档详细解释 `epoch_submit.sh` 脚本中 Solana 相关配置参数的含义。

## 核心概念

### 1. DISTRIBUTOR_PROGRAM_ID（程序 ID）

**定义：**
- Solana 链上部署的 Merkle Distributor 智能合约的程序 ID
- 类似于以太坊的合约地址，是合约在链上的唯一标识符
- 格式：Base58 编码的 32 字节公钥

**示例：**
```
DISTRIBUTOR_PROGRAM_ID="7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU"
```

**如何获取：**

#### 方法 1：部署时输出
```bash
# 使用 Anchor 部署合约
anchor deploy --provider.cluster mainnet-beta

# 输出示例：
# Program Id: 7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU
```

#### 方法 2：查看 Anchor.toml
```toml
[programs.mainnet-beta]
merkle_distributor = "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU"
```

#### 方法 3：使用 Anchor 命令
```bash
anchor keys list
# 输出：
# merkle_distributor: 7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU
```

#### 方法 4：查看部署日志
```bash
# 查看 Solana 程序账户
solana program show <program_id> --url mainnet-beta
```

**在代码中的使用：**
```javascript
// Node.js / Anchor
const PROGRAM_ID = new PublicKey(process.env.DISTRIBUTOR_PROGRAM_ID);
const program = new anchor.Program(idl, PROGRAM_ID, provider);
```

---

### 2. DISTRIBUTOR_ADDRESS（PDA 地址）

**定义：**
- Distributor 账户的 PDA (Program Derived Address) 地址
- PDA 是 Solana 中由程序派生的账户地址，用于存储合约状态数据
- 格式：Base58 编码的 32 字节公钥

**示例：**
```
DISTRIBUTOR_ADDRESS="9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM"
```

**PDA 的特点：**
- 由程序 ID 和种子（seeds）确定性生成
- 只有对应的程序可以签名 PDA 的交易
- 常用于存储合约的全局状态

**如何获取：**

#### 方法 1：初始化时输出
```rust
// Anchor 合约初始化代码
pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
    let distributor = &mut ctx.accounts.distributor;
    // distributor.key() 就是 PDA 地址
    msg!("Distributor PDA: {}", distributor.key());
    Ok(())
}
```

#### 方法 2：通过程序计算
```javascript
// JavaScript / TypeScript
import { PublicKey } from '@solana/web3.js';

const PROGRAM_ID = new PublicKey(DISTRIBUTOR_PROGRAM_ID);
const [distributorPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("distributor")],  // seeds
  PROGRAM_ID
);

console.log("Distributor PDA:", distributorPda.toString());
```

#### 方法 3：查询已部署的账户
```bash
# 使用 Solana CLI
solana account <distributor_address> --url mainnet-beta

# 或使用 Anchor
anchor account merkle_distributor <distributor_address>
```

**在代码中的使用：**
```javascript
// Node.js / Anchor
const DISTRIBUTOR_ADDRESS = new PublicKey(process.env.DISTRIBUTOR_ADDRESS);

// 在交易中使用
await program.methods
  .updateEpochRoot(epoch, root, leavesCount)
  .accounts({
    distributor: DISTRIBUTOR_ADDRESS,  // 使用 PDA 地址
    admin: adminKeypair.publicKey,
  })
  .rpc();
```

---

## 两者的区别

| 特性 | DISTRIBUTOR_PROGRAM_ID | DISTRIBUTOR_ADDRESS |
|------|----------------------|-------------------|
| **类型** | 程序账户（Program Account） | PDA 账户（Program Derived Address） |
| **作用** | 存储合约代码 | 存储合约状态数据（Merkle root、epoch 等） |
| **生成方式** | 部署时由密钥对生成 | 由程序 ID + seeds 确定性生成 |
| **可修改性** | 不可修改（代码） | 可修改（状态数据） |
| **类比** | 以太坊的合约地址 | 以太坊的合约存储槽 |

---

## 完整配置示例

### 开发环境（Devnet）
```bash
SOLANA_NETWORK="devnet"
SOLANA_RPC_URL="https://api.devnet.solana.com"
DISTRIBUTOR_PROGRAM_ID="YourDevnetProgramId..."
DISTRIBUTOR_ADDRESS="YourDevnetPdaAddress..."
```

### 测试环境（Testnet）
```bash
SOLANA_NETWORK="testnet"
SOLANA_RPC_URL="https://api.testnet.solana.com"
DISTRIBUTOR_PROGRAM_ID="YourTestnetProgramId..."
DISTRIBUTOR_ADDRESS="YourTestnetPdaAddress..."
```

### 生产环境（Mainnet）
```bash
SOLANA_NETWORK="mainnet-beta"
SOLANA_RPC_URL="https://api.mainnet-beta.solana.com"
DISTRIBUTOR_PROGRAM_ID="7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU"  # 示例
DISTRIBUTOR_ADDRESS="9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM"  # 示例
```

---

## 验证配置

### 验证 Program ID
```bash
# 检查程序是否已部署
solana program show $DISTRIBUTOR_PROGRAM_ID --url $SOLANA_NETWORK

# 应该输出：
# Program Id: 7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU
# Owner: BPFLoaderUpgradeab1e11111111111111111111111
# ProgramData Address: ...
# Authority: ...
```

### 验证 PDA 地址
```bash
# 检查账户是否存在
solana account $DISTRIBUTOR_ADDRESS --url $SOLANA_NETWORK

# 应该输出账户数据（如果已初始化）
```

### 使用 Anchor 验证
```bash
# 查看账户信息
anchor account merkle_distributor $DISTRIBUTOR_ADDRESS

# 或使用 IDL 查询
anchor idl parse --filepath target/idl/merkle_distributor.json
```

---

## 常见问题

### Q1: Program ID 和 PDA 地址可以相同吗？
**A:** 不可以。它们是不同类型的账户：
- Program ID 是程序账户，存储可执行代码
- PDA 是数据账户，存储状态数据

### Q2: 如何找到已部署合约的 Program ID？
**A:** 
1. 查看部署日志
2. 查看 Anchor.toml 配置
3. 使用 `anchor keys list` 命令
4. 在 Solana Explorer 上搜索合约名称

### Q3: PDA 地址是固定的吗？
**A:** 是的。PDA 由程序 ID 和 seeds 确定性生成，只要 seeds 不变，PDA 地址就固定。

### Q4: 可以在不同网络使用相同的 Program ID 吗？
**A:** 可以，但需要分别部署。每个网络（devnet/testnet/mainnet）的 Program ID 通常是不同的。

### Q5: 如何测试配置是否正确？
**A:** 
```bash
# 1. 验证程序存在
solana program show $DISTRIBUTOR_PROGRAM_ID --url $SOLANA_NETWORK

# 2. 验证 PDA 账户存在
solana account $DISTRIBUTOR_ADDRESS --url $SOLANA_NETWORK

# 3. 尝试调用只读方法（如果有）
# 使用 Anchor 或 web3.js 调用查询方法
```

---

## 相关资源

- [Solana Program ID 文档](https://docs.solana.com/developing/programming-model/accounts#program-derived-addresses)
- [Anchor PDA 文档](https://www.anchor-lang.com/docs/pdas)
- [Solana Explorer](https://explorer.solana.com/)
- [Anchor 官方文档](https://www.anchor-lang.com/)

---

## 配置检查清单

在运行 `epoch_submit.sh` 之前，请确认：

- [ ] `DISTRIBUTOR_PROGRAM_ID` 已设置为实际部署的程序 ID
- [ ] `DISTRIBUTOR_ADDRESS` 已设置为实际的 PDA 地址
- [ ] `SOLANA_NETWORK` 与部署环境匹配（devnet/testnet/mainnet-beta）
- [ ] `SOLANA_RPC_URL` 指向正确的 RPC 端点
- [ ] `ADMIN_KEYPAIR_PATH` 指向有效的管理员密钥文件
- [ ] 管理员账户有足够的 SOL 支付交易费用
- [ ] 已安装 Solana CLI 和 Anchor CLI（如需要）

完成以上检查后，即可运行脚本提交 epoch 到 Solana 链上。
