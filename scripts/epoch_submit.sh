#!/bin/bash
# Epoch Snapshot Submit Script
# 
# 此脚本用于：
# 1. 在 ICP 后端生成 epoch 快照（Merkle tree）
# 2. 将 Merkle root 提交到 Solana 链上的 Distributor 合约
# 
# 使用方法：
#   ./epoch_submit.sh <epoch_number>
# 
# 示例：
#   ./epoch_submit.sh 1

set -e

# ===== Configuration =====

# ICP Canister ID
BACKEND_CANISTER_ID="uxrrr-q7777-77774-qaaaq-cai"  # 需要替换为实际的 canister ID

# Solana Configuration
SOLANA_NETWORK="localnet"  # 或 "devnet" / "testnet" / "localnet"
# RPC URL 会根据 SOLANA_NETWORK 自动设置（如果未指定）
# SOLANA_RPC_URL="https://api.mainnet-beta.solana.com"  # 可以手动覆盖

# Solana Program (Anchor)
# DISTRIBUTOR_PROGRAM_ID: Solana 链上部署的 Merkle Distributor 智能合约的程序 ID
# 这是合约部署后获得的唯一地址（类似以太坊的合约地址）
# 格式：Base58 编码的 32 字节公钥，例如 "7DLja8cM4TMJodWBiM2VFesyJHH15hDhWimv4YJ7B2L5"
# 如何获取：
#   1. 部署合约后，Anchor 会输出 program ID
#   2. 或在 Anchor.toml 中查看 [programs.localnet] / [programs.mainnet] 配置
#   3. 或使用命令：anchor keys list
DISTRIBUTOR_PROGRAM_ID="7DLja8cM4TMJodWBiM2VFesyJHH15hDhWimv4YJ7B2L5"  # 需要替换为实际的 program ID

# 获取项目根目录（脚本所在目录的第三级上级目录）
# /Users/senyang/project/src/aio-base-backend/scripts/ -> /Users/senyang/project/
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
PMUG_DISTRIBUTOR_DIR="$PROJECT_ROOT/pmug-distributor"
IDL_PATH="$PMUG_DISTRIBUTOR_DIR/target/idl/merkle_distributor.json"

# 直接切换到项目根目录，这是执行 dfx 的最可靠方式
cd "$PROJECT_ROOT" || { echo "❌ 无法切换到项目根目录: $PROJECT_ROOT"; exit 1; }

# 定义 dfx 调用函数
dfx_call() {
    # 强制在根目录下执行，并显式指定网络（默认为 local）
    dfx "$@"
}

# Keypair (用于签名交易)
ADMIN_KEYPAIR_PATH="$HOME/.config/solana/id.json"  # 管理员密钥路径

# ===== Check Arguments =====

if [ $# -ne 1 ]; then
    echo "Usage: $0 <epoch_number>"
    echo "Example: $0 1"
    exit 1
fi

# ===== Validate Configuration =====

if [ "$BACKEND_CANISTER_ID" = "your-backend-canister-id" ]; then
    echo "❌ Error: Please set BACKEND_CANISTER_ID in the script"
    exit 1
fi

if [ "$DISTRIBUTOR_PROGRAM_ID" = "your-distributor-program-id" ]; then
    echo "❌ Error: Please set DISTRIBUTOR_PROGRAM_ID in the script"
    echo "You can find it in:"
    echo "  - Anchor.toml: [programs.$SOLANA_NETWORK]"
    echo "  - Or run: anchor keys list"
    exit 1
fi

if [ ! -f "$ADMIN_KEYPAIR_PATH" ]; then
    echo "❌ Error: Admin keypair not found at $ADMIN_KEYPAIR_PATH"
    echo "Please set ADMIN_KEYPAIR_PATH to your Solana keypair file"
    exit 1
fi

EPOCH=$1

# 根据网络自动设置 RPC URL（如果未手动指定）
if [ -z "$SOLANA_RPC_URL" ]; then
    case "$SOLANA_NETWORK" in
        localnet)
            SOLANA_RPC_URL="http://localhost:8899"
            ;;
        devnet)
            SOLANA_RPC_URL="https://api.devnet.solana.com"
            ;;
        testnet)
            SOLANA_RPC_URL="https://api.testnet.solana.com"
            ;;
        mainnet-beta|mainnet)
            SOLANA_RPC_URL="https://api.mainnet-beta.solana.com"
            ;;
        *)
            echo "❌ Error: Unknown network: $SOLANA_NETWORK"
            echo "Supported networks: localnet, devnet, testnet, mainnet-beta"
            exit 1
            ;;
    esac
fi

echo "============================================"
echo "Epoch Snapshot Submit Script"
echo "============================================"
echo "Epoch: $EPOCH"
echo "Backend Canister: $BACKEND_CANISTER_ID"
echo "Solana Network: $SOLANA_NETWORK"
echo "Solana RPC URL: $SOLANA_RPC_URL"
echo "============================================"
echo ""

# ===== Step 1: Build Epoch Snapshot on ICP =====

echo "[Step 1/3] Building epoch snapshot on ICP backend..."
echo "Canister ID: $BACKEND_CANISTER_ID"
echo "Epoch: $EPOCH"
echo ""

# 使用 dfx_call 确保在根目录执行
echo "正在调用 build_epoch_snapshot，请稍候..."
# 直接执行并捕获输出，但不屏蔽标准错误
if ! dfx_call canister call "$BACKEND_CANISTER_ID" build_epoch_snapshot "($EPOCH : nat64)"; then
    echo "❌ Error: Failed to build epoch snapshot"
    exit 1
fi

echo "✅ Epoch snapshot built successfully"
echo ""

# ===== Step 2: Get Epoch Metadata =====

echo "[Step 2/3] Fetching epoch metadata..."
echo ""

# 使用 dfx_call 确保在根目录执行
META_RESULT=$(dfx_call canister call "$BACKEND_CANISTER_ID" get_epoch_meta "($EPOCH : nat64)" 2>&1)

if echo "$META_RESULT" | grep -qi "null"; then
    echo "❌ Error: Epoch metadata not found"
    exit 1
fi

# 使用 Python 解析 Candid 输出（更可靠）
# 支持多种 root 格式：vec { 0x... } 或 blob "..."
export TEMP_META_RESULT="$META_RESULT"
PARSED_JSON=$(python3 << 'PYEOF'
import sys
import re
import os
import json

result = os.environ.get("TEMP_META_RESULT", "")

# 提取 Root
root_hex = ""
blob_match = re.search(r'root\s*=\s*blob\s*"([^"]+)"', result)
if blob_match:
    blob_str = blob_match.group(1)
    hex_val = ""
    i = 0
    while i < len(blob_str):
        if blob_str[i] == '\\':
            if i + 1 < len(blob_str):
                if re.match(r'[0-9a-fA-F]{2}', blob_str[i+1:i+3]):
                    hex_val += blob_str[i+1:i+3].lower()
                    i += 3
                elif blob_str[i+1] == '\\':
                    hex_val += format(ord('\\'), '02x')
                    i += 2
                elif blob_str[i+1] == '"':
                    hex_val += format(ord('"'), '02x')
                    i += 2
                elif blob_str[i+1] == 'n':
                    hex_val += format(ord('\n'), '02x')
                    i += 2
                elif blob_str[i+1] == 'r':
                    hex_val += format(ord('\r'), '02x')
                    i += 2
                elif blob_str[i+1] == 't':
                    hex_val += format(ord('\t'), '02x')
                    i += 2
                else:
                    hex_val += format(ord('\\'), '02x')
                    i += 1
            else:
                hex_val += format(ord('\\'), '02x')
                i += 1
        else:
            hex_val += format(ord(blob_str[i]), '02x')
            i += 1
    if len(hex_val) == 64:
        root_hex = hex_val

if not root_hex:
    root_match = re.search(r'root\s*=\s*vec\s*\{([^}]+)\}', result)
    if root_match:
        bytes_list = re.findall(r'0x([0-9a-fA-F]{2})', root_match.group(1))
        if len(bytes_list) == 32:
            root_hex = ''.join(bytes_list)

# 提取 Leaves Count
leaves_count = 0
count_match = re.search(r'leaves_count\s*=\s*([0-9_]+)', result)
if count_match:
    leaves_count = int(count_match.group(1).replace('_', ''))

print(json.dumps({"root": root_hex, "count": leaves_count}))
PYEOF
)
unset TEMP_META_RESULT

ROOT_HEX=$(echo "$PARSED_JSON" | python3 -c "import sys, json; print(json.load(sys.stdin)['root'])" | tail -n 1)
LEAVES_COUNT=$(echo "$PARSED_JSON" | python3 -c "import sys, json; print(json.load(sys.stdin)['count'])" | tail -n 1)

if [ -z "$ROOT_HEX" ] || [ "$LEAVES_COUNT" = "0" ]; then
    echo "❌ Error: Failed to parse metadata (Root: $ROOT_HEX, Count: $LEAVES_COUNT)"
    echo "Output: $META_RESULT"
    exit 1
fi

if [ $? -ne 0 ] || [ -z "$LEAVES_COUNT" ]; then
    echo "❌ Error: Failed to parse leaves_count from Candid output"
    echo "Output: $META_RESULT"
    exit 1
fi

echo "✅ Parsed epoch metadata:"
echo "  Root (hex): $ROOT_HEX"
echo "  Leaves Count: $LEAVES_COUNT"
echo ""

# ===== Step 3: Submit Root to Solana =====

echo "[Step 3/3] Submitting root to Solana..."
echo ""

# 检查 Solana CLI
if ! command -v solana &> /dev/null; then
    echo "❌ Error: Solana CLI not found. Please install it first."
    echo "Installation: sh -c \"\$(curl -sSfL https://release.solana.com/stable/install)\""
    exit 1
fi

# 检查 Anchor CLI (如果使用 Anchor)
if ! command -v anchor &> /dev/null; then
    echo "⚠️  Warning: Anchor CLI not found. You may need it for submitting transactions."
    echo "Installation: cargo install --git https://github.com/coral-xyz/anchor avm --locked --force"
fi

# 设置 Solana 网络
solana config set --url "$SOLANA_RPC_URL"

# 检查余额（仅对非本地网络）
if [ "$SOLANA_NETWORK" != "localnet" ]; then
    ADMIN_PUBKEY=$(solana address -k "$ADMIN_KEYPAIR_PATH" 2>/dev/null || echo "")
    if [ -n "$ADMIN_PUBKEY" ]; then
        BALANCE=$(solana balance "$ADMIN_KEYPAIR_PATH" 2>/dev/null | grep -oP '\d+\.\d+' || echo "0")
        echo "Admin wallet: $ADMIN_PUBKEY"
        echo "Balance: $BALANCE SOL"
        
        if command -v bc &> /dev/null; then
            if (( $(echo "$BALANCE < 0.01" | bc -l 2>/dev/null || echo 0) )); then
                echo "⚠️  Warning: Low balance. You may need more SOL for transaction fees."
            fi
        fi
    fi
fi

echo ""
echo "准备提交交易到 Solana..."
echo ""

# ===== Anchor Program Call =====

# 检查 IDL 文件是否存在
if [ ! -f "$IDL_PATH" ]; then
    echo "❌ Error: IDL file not found at $IDL_PATH"
    echo "Please build the Anchor program first:"
    echo "  cd $PMUG_DISTRIBUTOR_DIR && anchor build"
    exit 1
fi

# 检查是否安装了 Node.js
if ! command -v node &> /dev/null; then
    echo "❌ Error: Node.js not found. Please install it first."
    exit 1
fi

# 检查是否安装了必要的 npm 包
if [ ! -d "$PMUG_DISTRIBUTOR_DIR/node_modules/@coral-xyz/anchor" ]; then
    echo "⚠️  Warning: Anchor npm package not found. Installing dependencies..."
    cd "$PMUG_DISTRIBUTOR_DIR"
    if ! npm install; then
        echo "❌ Error: Failed to install npm dependencies"
        exit 1
    fi
else
    # 即使存在，也可能版本不兼容或损坏。尝试链接
    cd "$PMUG_DISTRIBUTOR_DIR"
fi

# 创建临时 Node.js 脚本 (放在 pmug-distributor 目录下以确保能找到 node_modules)
SUBMIT_SCRIPT="$PMUG_DISTRIBUTOR_DIR/submit_epoch_${EPOCH}_$$.js"

cat > "$SUBMIT_SCRIPT" << 'NODEEOF'
const anchor = require('@coral-xyz/anchor');
const { PublicKey, SystemProgram, Keypair } = require('@solana/web3.js');
const fs = require('fs');

// 从环境变量获取配置
const PROGRAM_ID = new PublicKey(process.env.DISTRIBUTOR_PROGRAM_ID);
const RPC_URL = process.env.SOLANA_RPC_URL;
const ADMIN_KEYPAIR_PATH = process.env.ADMIN_KEYPAIR_PATH;
const IDL_PATH = process.env.IDL_PATH;
const EPOCH = BigInt(process.env.EPOCH);
const ROOT_HEX = process.env.ROOT_HEX;
const LEAVES_COUNT = parseInt(process.env.LEAVES_COUNT);
const SOLANA_NETWORK = process.env.SOLANA_NETWORK;

// 计算 PDA 的函数
function getDistributorPda(programId) {
  return PublicKey.findProgramAddressSync([Buffer.from("distributor")], programId);
}

function getEpochDataPda(programId, distributorPda, epoch) {
  const epochBuf = Buffer.allocUnsafe(8);
  epochBuf.writeBigUInt64LE(epoch);
  return PublicKey.findProgramAddressSync(
    [Buffer.from("epoch"), distributorPda.toBuffer(), epochBuf],
    programId
  );
}

async function main() {
  console.log('🔄 准备提交 Epoch Root 到 Solana...');
  console.log('Epoch:', EPOCH.toString());
  console.log('Root (hex):', ROOT_HEX);
  console.log('Leaves Count:', LEAVES_COUNT);
  console.log('Program ID:', PROGRAM_ID.toString());
  console.log('');

  // 连接到 Solana
  const connection = new anchor.web3.Connection(RPC_URL, 'confirmed');
  
  // 加载管理员密钥
  const keypairData = JSON.parse(fs.readFileSync(ADMIN_KEYPAIR_PATH, 'utf-8'));
  const adminKeypair = Keypair.fromSecretKey(Uint8Array.from(keypairData));
  
  console.log('Admin wallet:', adminKeypair.publicKey.toString());
  
  // 创建 Provider
  const wallet = new anchor.Wallet(adminKeypair);
  const provider = new anchor.AnchorProvider(connection, wallet, {
    commitment: 'confirmed',
  });
  
  // 加载 IDL
  const idl = JSON.parse(fs.readFileSync(IDL_PATH, 'utf-8'));
  
  // 修复 IDL：Anchor 0.32.1 对 0.30.1 的 IDL 兼容性处理
  // 确保 accounts 中的每个项都有对应的 type 引用
  if (idl.accounts && idl.types) {
    idl.accounts.forEach(acc => {
      if (!acc.type) {
        const typeDef = idl.types.find(t => t.name === acc.name);
        if (typeDef) {
          acc.type = typeDef.name;
        }
      }
    });
  }

  // 在 Anchor 0.30+ 中，如果 IDL 包含地址，只需传入 idl 和 provider
  const program = new anchor.Program(idl, provider);
  
  // 计算 PDA
  const [distributor] = getDistributorPda(program.programId);
  const [epochData] = getEpochDataPda(program.programId, distributor, EPOCH);
  
  console.log('📍 PDA 地址:');
  console.log('  Distributor:', distributor.toString());
  console.log('  Epoch Data:', epochData.toString());
  console.log('');
  
  // 转换 root hex 为数组
  if (ROOT_HEX.length !== 64) {
    throw new Error(`Root must be 64 hex characters (32 bytes), got ${ROOT_HEX.length}`);
  }
  const rootBuffer = Buffer.from(ROOT_HEX, 'hex');
  const root = Array.from(rootBuffer);
  
  console.log('🚀 发送更新交易...');
  
  try {
    // 检查账户是否已存在，避免 init 冲突
    const existingAccount = await connection.getAccountInfo(epochData);
    if (existingAccount) {
      console.log('⚠️  Epoch Data 账户已存在，检查内容...');
      try {
        const data = await program.account.epochData.fetch(epochData);
        const onChainRoot = Buffer.from(data.root).toString('hex');
        if (onChainRoot === ROOT_HEX) {
          console.log('✅ 链上 Root 与本地完全匹配，跳过提交。');
          return;
        } else {
          console.log('❌ 冲突：Epoch 已存在且包含不同的 Root！');
          console.log('  链上:', onChainRoot);
          console.log('  本地:', ROOT_HEX);
          process.exit(1);
        }
      } catch (e) {
        console.log('⚠️  无法读取账户内容，可能未完全初始化。尝试继续提交...');
      }
    }

    const tx = await program.methods
      .updateEpochRoot(new anchor.BN(EPOCH.toString()), root, LEAVES_COUNT)
      .accounts({
        distributor: distributor,
        admin: adminKeypair.publicKey,
        epochData: epochData,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
    
    console.log('✅ 交易已提交！');
    console.log('交易签名:', tx);
    
    // 根据网络生成浏览器链接
    let explorerUrl;
    if (SOLANA_NETWORK === 'localnet') {
      explorerUrl = `http://localhost:8899/tx/${tx}`;
    } else if (SOLANA_NETWORK === 'devnet') {
      explorerUrl = `https://explorer.solana.com/tx/${tx}?cluster=devnet`;
    } else {
      explorerUrl = `https://explorer.solana.com/tx/${tx}`;
    }
    console.log('浏览器查看:', explorerUrl);
    
    // 等待确认
    console.log('⏳ 等待交易确认...');
    await connection.confirmTransaction(tx, 'confirmed');
    console.log('✅ 交易已确认！');
    
    // 验证更新
    console.log('');
    console.log('🔍 验证更新结果...');
    const epochAccount = await program.account.epochData.fetch(epochData);
    console.log('✅ Epoch Root 更新成功！');
    console.log('  Epoch:', epochAccount.epoch.toString());
    console.log('  Root:', Buffer.from(epochAccount.root).toString('hex'));
    console.log('  Leaves Count:', epochAccount.leavesCount.toString());
    console.log('  Updated At:', new Date(epochAccount.updatedAt.toNumber() * 1000).toISOString());
    
  } catch (error) {
    console.error('❌ 错误:', error);
    if (error.logs) {
      console.error('交易日志:');
      error.logs.forEach(log => console.error('  ', log));
    }
    process.exit(1);
  }
}

main()
  .then(() => process.exit(0))
  .catch((err) => {
    console.error('❌ 未处理的错误:', err);
    process.exit(1);
  });
NODEEOF

# 设置环境变量并执行脚本
export DISTRIBUTOR_PROGRAM_ID="$DISTRIBUTOR_PROGRAM_ID"
export SOLANA_RPC_URL="$SOLANA_RPC_URL"
export ADMIN_KEYPAIR_PATH="$ADMIN_KEYPAIR_PATH"
export IDL_PATH="$IDL_PATH"
export EPOCH="$EPOCH"
export ROOT_HEX="$ROOT_HEX"
export LEAVES_COUNT="$LEAVES_COUNT"
export SOLANA_NETWORK="$SOLANA_NETWORK"

# 切换到 pmug-distributor 目录以使用 node_modules
cd "$PMUG_DISTRIBUTOR_DIR"

# 运行脚本
if ! node "$SUBMIT_SCRIPT"; then
    echo ""
    echo "❌ Error: Failed to submit epoch root to Solana"
    rm -f "$SUBMIT_SCRIPT"
    exit 1
fi

# 清理临时文件
rm -f "$SUBMIT_SCRIPT"

# ===== Step 4: Summary =====

echo ""
echo "============================================"
echo "✅ Epoch Submission Complete"
echo "============================================"
echo "Epoch: $EPOCH"
echo "Leaves Count: $LEAVES_COUNT"
echo "Root (hex): $ROOT_HEX"
echo "Status: ✅ Successfully submitted to Solana"
echo "============================================"
echo ""
echo "📝 Next steps:"
echo "1. Verify the transaction on Solana Explorer"
echo "2. Users can now claim their rewards via the frontend"
echo ""
