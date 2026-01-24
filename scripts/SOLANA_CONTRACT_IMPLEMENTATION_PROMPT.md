# Solana Merkle Distributor 合约实现提示词

## 项目概述

你需要在 Solana 链上实现一个 Merkle Distributor 智能合约，用于分发任务奖励（PMUG 代币）。此合约必须与 ICP 后端（`src/aio-base-backend/src/task_rewards.rs`）完全兼容。

## 核心要求

### 1. Merkle Tree 规范（CRITICAL - 必须与后端一致）

**Leaf Hash 计算（强制）：**
```
leaf_hash = SHA256(
    epoch (u64 little-endian 8 bytes) ||
    index (u32 little-endian 4 bytes) ||
    wallet_pubkey (32 bytes, base58 decoded) ||
    amount (u64 little-endian 8 bytes)
)
```

**Parent Hash 计算（强制 - 排序哈希）：**
```
parent_hash = SHA256(
    min(left_hash, right_hash) ||
    max(left_hash, right_hash)
)
```

**重要说明：**
- 使用排序哈希，proof 中不需要方向位
- 所有数值使用 little-endian 编码
- wallet_pubkey 是 base58 解码后的 32 字节原始公钥
- 必须与后端 `task_rewards.rs` 中的实现完全一致

### 2. 合约功能要求

1. **初始化 Distributor**
   - 设置管理员
   - 设置代币 mint 地址
   - 创建 Distributor PDA

2. **更新 Epoch Root**（仅管理员）
   - 接收 epoch 编号、Merkle root、叶子数量
   - 存储到 Distributor 账户
   - 防止重复更新同一 epoch

3. **Claim 奖励**（用户调用）
   - 验证 Merkle proof
   - 检查是否已 claim（使用 ClaimStatus PDA）
   - 转账 PMUG 代币给用户
   - 记录 claim 状态

4. **查询功能**
   - 查询 epoch 信息
   - 查询用户 claim 状态
   - 查询可领取金额

## 项目初始化步骤

### Step 1: 创建 Anchor 项目

```bash
# 安装 Anchor（如果未安装）
cargo install --git https://github.com/coral-xyz/anchor avm --locked --force
avm install latest
avm use latest

# 创建新项目
anchor init merkle-distributor
cd merkle-distributor

# 项目结构
merkle-distributor/
├── Anchor.toml
├── Cargo.toml
├── programs/
│   └── merkle-distributor/
│       ├── Cargo.toml
│       └── src/
│           └── lib.rs
├── tests/
│   └── merkle-distributor.ts
└── migrations/
    └── deploy.ts
```

### Step 2: 更新 Anchor.toml

```toml
[features]
resolution = true
skip-lint = false

[programs.localnet]
merkle_distributor = "YOUR_PROGRAM_ID"

[programs.devnet]
merkle_distributor = "YOUR_PROGRAM_ID"

[programs.mainnet]
merkle_distributor = "YOUR_PROGRAM_ID"

[registry]
url = "https://api.apr.dev"

[provider]
cluster = "Localnet"
wallet = "~/.config/solana/id.json"

[scripts]
test = "yarn run ts-mocha -p ./tsconfig.json -t 1000000 tests/**/*.ts"
```

### Step 3: 安装依赖

```bash
# 在项目根目录
npm install @coral-xyz/anchor @solana/web3.js @solana/spl-token
npm install --save-dev @types/node typescript ts-mocha mocha
```

## 完整合约实现

### lib.rs - 主合约文件

```rust
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};
use sha2::{Sha256, Digest};

declare_id!("YOUR_PROGRAM_ID");  // 部署后更新

#[program]
pub mod merkle_distributor {
    use super::*;

    /// 初始化 Distributor
    /// 
    /// # Accounts
    /// - distributor: Distributor PDA (seeds: ["distributor"])
    /// - admin: 管理员账户（签名者）
    /// - token_mint: PMUG 代币 mint 地址
    /// - vault: 代币金库账户（PDA，存储待分发的代币）
    /// - token_program: SPL Token 程序
    /// - system_program: System 程序
    pub fn initialize(
        ctx: Context<Initialize>,
        token_mint: Pubkey,
    ) -> Result<()> {
        let distributor = &mut ctx.accounts.distributor;
        distributor.admin = ctx.accounts.admin.key();
        distributor.token_mint = token_mint;
        distributor.vault = ctx.accounts.vault.key();
        distributor.bump = ctx.bumps.distributor;
        
        msg!("Distributor initialized");
        msg!("Admin: {}", distributor.admin);
        msg!("Token Mint: {}", distributor.token_mint);
        msg!("Vault: {}", distributor.vault);
        
        Ok(())
    }

    /// 更新 Epoch Root（仅管理员）
    /// 
    /// # Accounts
    /// - distributor: Distributor PDA
    /// - admin: 管理员账户（签名者，必须是 distributor.admin）
    /// 
    /// # Arguments
    /// - epoch: Epoch 编号
    /// - root: Merkle root (32 bytes)
    /// - leaves_count: 叶子节点数量
    pub fn update_epoch_root(
        ctx: Context<UpdateEpochRoot>,
        epoch: u64,
        root: [u8; 32],
        leaves_count: u32,
    ) -> Result<()> {
        // 验证管理员权限
        require!(
            ctx.accounts.admin.key() == ctx.accounts.distributor.admin,
            ErrorCode::Unauthorized
        );

        // 检查 epoch 是否已存在
        let epoch_data = &mut ctx.accounts.epoch_data;
        require!(
            epoch_data.epoch == 0 || epoch_data.epoch != epoch,
            ErrorCode::EpochAlreadyExists
        );

        // 更新 epoch 数据
        epoch_data.epoch = epoch;
        epoch_data.root = root;
        epoch_data.leaves_count = leaves_count;
        epoch_data.updated_at = Clock::get()?.unix_timestamp as u64;

        msg!("Epoch {} root updated", epoch);
        msg!("Root: {:?}", root);
        msg!("Leaves count: {}", leaves_count);

        Ok(())
    }

    /// Claim 奖励（用户调用）
    /// 
    /// # Accounts
    /// - distributor: Distributor PDA
    /// - epoch_data: Epoch 数据账户（PDA，seeds: ["epoch", epoch])
    /// - claim_status: Claim 状态账户（PDA，seeds: ["claim", epoch, claimer])
    /// - claimer: 领取者账户（签名者）
    /// - claimer_token_account: 领取者的代币账户
    /// - vault: 代币金库账户
    /// - token_program: SPL Token 程序
    /// 
    /// # Arguments
    /// - epoch: Epoch 编号
    /// - index: 在 Merkle tree 中的索引
    /// - amount: 奖励金额（PMUG 最小单位）
    /// - proof: Merkle proof (Vec<[u8; 32]>)
    pub fn claim(
        ctx: Context<Claim>,
        epoch: u64,
        index: u32,
        amount: u64,
        proof: Vec<[u8; 32]>,
    ) -> Result<()> {
        let distributor = &ctx.accounts.distributor;
        let epoch_data = &ctx.accounts.epoch_data;
        let claim_status = &mut ctx.accounts.claim_status;
        let claimer = &ctx.accounts.claimer;

        // 验证 epoch 存在
        require!(
            epoch_data.epoch == epoch,
            ErrorCode::InvalidEpoch
        );

        // 验证未重复 claim
        require!(
            !claim_status.claimed,
            ErrorCode::AlreadyClaimed
        );

        // 计算 leaf hash
        let leaf_hash = compute_leaf_hash(
            epoch,
            index,
            &claimer.key().to_bytes(),
            amount,
        );

        // 验证 Merkle proof
        let root = verify_merkle_proof(
            &leaf_hash,
            &proof,
            &epoch_data.root,
            index,
            epoch_data.leaves_count,
        )?;

        require!(
            root == epoch_data.root,
            ErrorCode::InvalidProof
        );

        // 标记为已 claim
        claim_status.claimed = true;
        claim_status.claimed_at = Clock::get()?.unix_timestamp as u64;
        claim_status.amount = amount;

        // 转账代币
        let seeds = &[
            b"distributor",
            &[distributor.bump],
        ];
        let signer = &[&seeds[..]];

        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.vault.to_account_info(),
                    to: ctx.accounts.claimer_token_account.to_account_info(),
                    authority: ctx.accounts.distributor.to_account_info(),
                },
                signer,
            ),
            amount,
        )?;

        msg!("Claim successful");
        msg!("Claimer: {}", claimer.key());
        msg!("Amount: {}", amount);
        msg!("Epoch: {}", epoch);

        Ok(())
    }

    /// 查询 epoch 信息（只读）
    pub fn get_epoch_info(
        ctx: Context<GetEpochInfo>,
        epoch: u64,
    ) -> Result<EpochInfo> {
        let epoch_data = &ctx.accounts.epoch_data;
        require!(
            epoch_data.epoch == epoch,
            ErrorCode::InvalidEpoch
        );

        Ok(EpochInfo {
            epoch: epoch_data.epoch,
            root: epoch_data.root,
            leaves_count: epoch_data.leaves_count,
            updated_at: epoch_data.updated_at,
        })
    }
}

// ===== Accounts =====

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = admin,
        space = 8 + Distributor::LEN,
        seeds = [b"distributor"],
        bump
    )]
    pub distributor: Account<'info, Distributor>,

    #[account(mut)]
    pub admin: Signer<'info>,

    /// CHECK: Token mint address (验证但不读取)
    pub token_mint: UncheckedAccount<'info>,

    #[account(
        init,
        payer = admin,
        token::mint = token_mint,
        token::authority = distributor,
        seeds = [b"vault", distributor.key().as_ref()],
        bump
    )]
    pub vault: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(epoch: u64)]
pub struct UpdateEpochRoot<'info> {
    #[account(
        mut,
        seeds = [b"distributor"],
        bump = distributor.bump,
        has_one = admin @ ErrorCode::Unauthorized
    )]
    pub distributor: Account<'info, Distributor>,

    pub admin: Signer<'info>,

    #[account(
        init_if_needed,
        payer = admin,
        space = 8 + EpochData::LEN,
        seeds = [b"epoch", &epoch.to_le_bytes()],
        bump
    )]
    pub epoch_data: Account<'info, EpochData>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(epoch: u64)]
pub struct Claim<'info> {
    #[account(
        seeds = [b"distributor"],
        bump = distributor.bump
    )]
    pub distributor: Account<'info, Distributor>,

    #[account(
        seeds = [b"epoch", &epoch.to_le_bytes()],
        bump
    )]
    pub epoch_data: Account<'info, EpochData>,

    #[account(
        init_if_needed,
        payer = claimer,
        space = 8 + ClaimStatus::LEN,
        seeds = [b"claim", &epoch.to_le_bytes(), claimer.key().as_ref()],
        bump
    )]
    pub claim_status: Account<'info, ClaimStatus>,

    pub claimer: Signer<'info>,

    #[account(mut)]
    pub claimer_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [b"vault", distributor.key().as_ref()],
        bump
    )]
    pub vault: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(epoch: u64)]
pub struct GetEpochInfo<'info> {
    #[account(
        seeds = [b"epoch", &epoch.to_le_bytes()],
        bump
    )]
    pub epoch_data: Account<'info, EpochData>,
}

// ===== State Accounts =====

#[account]
pub struct Distributor {
    pub admin: Pubkey,           // 32 bytes
    pub token_mint: Pubkey,      // 32 bytes
    pub vault: Pubkey,           // 32 bytes
    pub bump: u8,                // 1 byte
}

impl Distributor {
    pub const LEN: usize = 32 + 32 + 32 + 1;
}

#[account]
pub struct EpochData {
    pub epoch: u64,              // 8 bytes
    pub root: [u8; 32],          // 32 bytes
    pub leaves_count: u32,       // 4 bytes
    pub updated_at: u64,         // 8 bytes
}

impl EpochData {
    pub const LEN: usize = 8 + 32 + 4 + 8;
}

#[account]
pub struct ClaimStatus {
    pub epoch: u64,              // 8 bytes
    pub claimer: Pubkey,         // 32 bytes
    pub claimed: bool,            // 1 byte
    pub amount: u64,              // 8 bytes
    pub claimed_at: u64,         // 8 bytes
}

impl ClaimStatus {
    pub const LEN: usize = 8 + 32 + 1 + 8 + 8;
}

// ===== Return Types =====

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct EpochInfo {
    pub epoch: u64,
    pub root: [u8; 32],
    pub leaves_count: u32,
    pub updated_at: u64,
}

// ===== Merkle Tree Functions =====

/// 计算 leaf hash（必须与后端一致）
/// 
/// leaf_hash = SHA256(
///     epoch (u64 le) ||
///     index (u32 le) ||
///     wallet_pubkey (32 bytes) ||
///     amount (u64 le)
/// )
fn compute_leaf_hash(
    epoch: u64,
    index: u32,
    wallet_pubkey: &[u8; 32],
    amount: u64,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(&epoch.to_le_bytes());
    hasher.update(&index.to_le_bytes());
    hasher.update(wallet_pubkey);
    hasher.update(&amount.to_le_bytes());
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

/// 验证 Merkle proof（使用排序哈希）
/// 
/// parent = SHA256(min(left, right) || max(left, right))
fn verify_merkle_proof(
    leaf: &[u8; 32],
    proof: &[[u8; 32]],
    root: &[u8; 32],
    index: u32,
    leaves_count: u32,
) -> Result<[u8; 32]> {
    let mut current_hash = *leaf;
    let mut current_index = index as usize;

    for sibling_hash in proof {
        // 计算父节点（排序哈希）
        let (left, right) = if current_index % 2 == 0 {
            (current_hash, *sibling_hash)
        } else {
            (*sibling_hash, current_hash)
        };

        // 排序：min(left, right) || max(left, right)
        let (min_hash, max_hash) = if left <= right {
            (left, right)
        } else {
            (right, left)
        };

        let mut hasher = Sha256::new();
        hasher.update(&min_hash);
        hasher.update(&max_hash);
        let result = hasher.finalize();
        current_hash.copy_from_slice(&result);

        current_index /= 2;
    }

    // 验证最终 hash 是否等于 root
    if current_hash != *root {
        return Err(ErrorCode::InvalidProof.into());
    }

    Ok(current_hash)
}

// ===== Error Codes =====

#[error_code]
pub enum ErrorCode {
    #[msg("Unauthorized: Only admin can perform this action")]
    Unauthorized,

    #[msg("Epoch already exists")]
    EpochAlreadyExists,

    #[msg("Invalid epoch")]
    InvalidEpoch,

    #[msg("Invalid Merkle proof")]
    InvalidProof,

    #[msg("Already claimed")]
    AlreadyClaimed,

    #[msg("Insufficient funds in vault")]
    InsufficientFunds,
}
```

### Cargo.toml - 依赖配置

```toml
[package]
name = "merkle-distributor"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "lib"]
name = "merkle_distributor"

[features]
no-entrypoint = []
no-idl = []
no-log-ix-name = []
cpi = ["no-entrypoint"]
default = []

[dependencies]
anchor-lang = "0.29.0"
anchor-spl = "0.29.0"
sha2 = "0.10"
```

## 测试文件

### tests/merkle-distributor.ts

```typescript
import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { MerkleDistributor } from "../target/types/merkle_distributor";
import { 
  TOKEN_PROGRAM_ID, 
  createMint, 
  createAccount,
  mintTo,
  getAccount,
} from "@solana/spl-token";
import { 
  Keypair, 
  PublicKey, 
  SystemProgram,
  LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import { sha256 } from "@noble/hashes/sha256";
import { expect } from "chai";

describe("merkle-distributor", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.MerkleDistributor as Program<MerkleDistributor>;
  
  let admin: Keypair;
  let tokenMint: PublicKey;
  let distributor: PublicKey;
  let vault: PublicKey;
  let user: Keypair;
  let userTokenAccount: PublicKey;

  before(async () => {
    // 创建管理员
    admin = Keypair.generate();
    const airdropAdmin = await provider.connection.requestAirdrop(
      admin.publicKey,
      2 * LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(airdropAdmin);

    // 创建用户
    user = Keypair.generate();
    const airdropUser = await provider.connection.requestAirdrop(
      user.publicKey,
      2 * LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(airdropUser);

    // 创建代币 mint
    tokenMint = await createMint(
      provider.connection,
      admin,
      admin.publicKey,
      null,
      6 // 6 位小数
    );

    // 创建用户代币账户
    userTokenAccount = await createAccount(
      provider.connection,
      user,
      tokenMint,
      user.publicKey
    );

    // 计算 PDA
    [distributor] = PublicKey.findProgramAddressSync(
      [Buffer.from("distributor")],
      program.programId
    );

    [vault] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), distributor.toBuffer()],
      program.programId
    );
  });

  it("Initializes the distributor", async () => {
    const tx = await program.methods
      .initialize(tokenMint)
      .accounts({
        distributor,
        admin: admin.publicKey,
        tokenMint,
        vault,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([admin])
      .rpc();

    console.log("Initialize transaction:", tx);

    // 验证 distributor 账户
    const distributorAccount = await program.account.distributor.fetch(distributor);
    expect(distributorAccount.admin.toString()).to.equal(admin.publicKey.toString());
    expect(distributorAccount.tokenMint.toString()).to.equal(tokenMint.toString());
  });

  it("Updates epoch root", async () => {
    const epoch = new anchor.BN(1);
    const root = Buffer.alloc(32, 1); // 示例 root
    const leavesCount = new anchor.BN(10);

    const [epochData] = PublicKey.findProgramAddressSync(
      [Buffer.from("epoch"), epoch.toArrayLike(Buffer, "le", 8)],
      program.programId
    );

    const tx = await program.methods
      .updateEpochRoot(epoch, Array.from(root), leavesCount)
      .accounts({
        distributor,
        admin: admin.publicKey,
        epochData,
        systemProgram: SystemProgram.programId,
      })
      .signers([admin])
      .rpc();

    console.log("Update epoch root transaction:", tx);

    // 验证 epoch 数据
    const epochAccount = await program.account.epochData.fetch(epochData);
    expect(epochAccount.epoch.toNumber()).to.equal(1);
    expect(epochAccount.leavesCount.toNumber()).to.equal(10);
  });

  it("Claims reward with valid proof", async () => {
    // 1. 准备测试数据
    const epoch = new anchor.BN(1);
    const index = new anchor.BN(0);
    const amount = new anchor.BN(100_000_000); // 100 PMUG

    // 2. 计算 leaf hash（与后端一致）
    const leafHash = computeLeafHash(
      epoch.toNumber(),
      index.toNumber(),
      user.publicKey.toBytes(),
      amount.toNumber()
    );

    // 3. 生成简单的 proof（实际应从后端获取）
    const proof: number[][] = []; // 这里需要实际的 proof

    // 4. 向 vault 存入代币
    await mintTo(
      provider.connection,
      admin,
      tokenMint,
      vault,
      admin,
      amount.toNumber()
    );

    // 5. 计算 PDA
    const [epochData] = PublicKey.findProgramAddressSync(
      [Buffer.from("epoch"), epoch.toArrayLike(Buffer, "le", 8)],
      program.programId
    );

    const [claimStatus] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("claim"),
        epoch.toArrayLike(Buffer, "le", 8),
        user.publicKey.toBuffer(),
      ],
      program.programId
    );

    // 6. 执行 claim
    const tx = await program.methods
      .claim(epoch, index, amount, proof)
      .accounts({
        distributor,
        epochData,
        claimStatus,
        claimer: user.publicKey,
        claimerTokenAccount: userTokenAccount,
        vault,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([user])
      .rpc();

    console.log("Claim transaction:", tx);

    // 7. 验证 claim 状态
    const claimAccount = await program.account.claimStatus.fetch(claimStatus);
    expect(claimAccount.claimed).to.be.true;
    expect(claimAccount.amount.toNumber()).to.equal(amount.toNumber());

    // 8. 验证代币已转账
    const userBalance = await getAccount(provider.connection, userTokenAccount);
    expect(Number(userBalance.amount)).to.equal(amount.toNumber());
  });
});

// 辅助函数：计算 leaf hash（与后端一致）
function computeLeafHash(
  epoch: number,
  index: number,
  walletPubkey: Uint8Array,
  amount: number
): Uint8Array {
  const epochBytes = Buffer.allocUnsafe(8);
  epochBytes.writeBigUInt64LE(BigInt(epoch), 0);

  const indexBytes = Buffer.allocUnsafe(4);
  indexBytes.writeUInt32LE(index, 0);

  const amountBytes = Buffer.allocUnsafe(8);
  amountBytes.writeBigUInt64LE(BigInt(amount), 0);

  const data = Buffer.concat([
    epochBytes,
    indexBytes,
    Buffer.from(walletPubkey),
    amountBytes,
  ]);

  return sha256(data);
}
```

## 部署步骤

### 1. 构建合约

```bash
anchor build
```

### 2. 获取 Program ID

```bash
# 查看生成的 Program ID
anchor keys list

# 或查看 target/deploy/merkle_distributor-keypair.json
```

### 3. 更新 Program ID

```bash
# 在 lib.rs 中更新
declare_id!("YOUR_PROGRAM_ID");

# 在 Anchor.toml 中更新
[programs.localnet]
merkle_distributor = "YOUR_PROGRAM_ID"
```

### 4. 重新构建

```bash
anchor build
```

### 5. 部署到 Devnet

```bash
# 设置网络
solana config set --url devnet

# 获取测试 SOL
solana airdrop 2 $(solana address)

# 部署
anchor deploy --provider.cluster devnet
```

### 6. 部署到 Mainnet

```bash
# 设置网络
solana config set --url mainnet-beta

# 确保有足够的 SOL 支付部署费用
solana balance

# 部署（谨慎操作！）
anchor deploy --provider.cluster mainnet-beta
```

## 与后端集成

### 1. 更新 epoch_submit.sh

部署后，更新脚本中的配置：

```bash
DISTRIBUTOR_PROGRAM_ID="你的实际 Program ID"
DISTRIBUTOR_ADDRESS="你的 Distributor PDA 地址"
```

### 2. 验证兼容性

确保以下内容与后端一致：

- ✅ Leaf hash 计算方式
- ✅ Parent hash 计算方式（排序哈希）
- ✅ 字节序（little-endian）
- ✅ Wallet pubkey 格式（32 字节）

### 3. 测试端到端流程

1. 后端生成 epoch 快照
2. 脚本提交 root 到 Solana
3. 用户获取 claim ticket
4. 前端调用合约 claim
5. 验证代币到账

## 安全检查清单

- [ ] 管理员权限验证
- [ ] Epoch 防重复更新
- [ ] Claim 防重复（ClaimStatus PDA）
- [ ] Merkle proof 验证
- [ ] 代币余额检查
- [ ] 错误处理完善
- [ ] 事件日志记录

## 常见问题

### Q: 如何获取实际的 Merkle proof？

A: 从 ICP 后端调用 `get_claim_ticket(wallet)` API，返回的 `proof` 字段就是 Merkle proof。

### Q: Wallet pubkey 格式是什么？

A: 是 Solana 公钥的原始 32 字节，不是 base58 字符串。在 Anchor 中使用 `pubkey.to_bytes()` 获取。

### Q: 如何测试 Merkle proof？

A: 使用后端生成的测试数据，或创建简单的 Merkle tree 进行单元测试。

### Q: 代币 mint 地址在哪里？

A: 这是 PMUG 代币的 mint 地址，需要从代币部署信息中获取。

## 下一步

1. ✅ 实现合约代码
2. ✅ 编写测试
3. ✅ 部署到 Devnet
4. ✅ 与后端集成测试
5. ✅ 部署到 Mainnet
6. ✅ 更新运营脚本配置

## 参考资源

- [Anchor 官方文档](https://www.anchor-lang.com/)
- [Solana Cookbook](https://solanacookbook.com/)
- [SPL Token 文档](https://spl.solana.com/token)
- 后端实现：`src/aio-base-backend/src/task_rewards.rs`

---

**重要提醒：** 在部署到主网之前，务必：
1. 完整测试所有功能
2. 进行安全审计
3. 验证与后端的兼容性
4. 准备应急方案
