#!/bin/bash
# Epoch Snapshot Submit Script
# 
# 此脚本用于：
# 1. 在 ICP 后端生成 epoch 快照（Merkle tree）
# 2. 将 Merkle root 提交到 Solana 链上的 Distributor 合约
# 3. (可选) 验证 Claim 功能
# 
# 使用方法：
#   ./epoch_submit.sh <epoch_number> [test_wallet] [test_wallet_keypair]
# 
# 示例：
#   ./epoch_submit.sh 1
#   ./epoch_submit.sh 1 "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU"
# 
# 备注：如果要验证 Claim，必须提供 test_wallet 及其对应的 keypair 文件（默认为 ~/.config/solana/id.json）

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
DISTRIBUTOR_PROGRAM_ID="7DLja8cM4TMJodWBiM2VFesyJHH15hDhWimv4YJ7B2L5"  # 需要替换为实际的 program ID

# 获取项目根目录
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
PMUG_DISTRIBUTOR_DIR="$PROJECT_ROOT/pmug-distributor"
IDL_PATH="$PMUG_DISTRIBUTOR_DIR/target/idl/merkle_distributor.json"

# 直接切换到项目根目录
cd "$PROJECT_ROOT" || { echo "❌ 无法切换到项目根目录: $PROJECT_ROOT"; exit 1; }

# 定义 dfx 调用函数
dfx_call() {
    dfx "$@"
}

# Keypair (用于签名交易)
ADMIN_KEYPAIR_PATH="$HOME/.config/solana/id.json"  # 管理员密钥路径

# ===== Check Arguments =====

if [ $# -lt 1 ]; then
    echo "Usage: $0 <epoch_number> [test_wallet] [test_wallet_keypair]"
    echo "Example: $0 1"
    exit 1
fi

EPOCH=$1
TEST_WALLET=$2
TEST_WALLET_KEYPAIR=${3:-$ADMIN_KEYPAIR_PATH}

# ===== Validate Configuration =====

if [ "$BACKEND_CANISTER_ID" = "your-backend-canister-id" ]; then
    echo "❌ Error: Please set BACKEND_CANISTER_ID in the script"
    exit 1
fi

if [ "$DISTRIBUTOR_PROGRAM_ID" = "your-distributor-program-id" ]; then
    echo "❌ Error: Please set DISTRIBUTOR_PROGRAM_ID in the script"
    exit 1
fi

if [ ! -f "$ADMIN_KEYPAIR_PATH" ]; then
    echo "❌ Error: Admin keypair not found at $ADMIN_KEYPAIR_PATH"
    exit 1
fi

if [ -n "$TEST_WALLET" ] && [ ! -f "$TEST_WALLET_KEYPAIR" ]; then
    echo "❌ Error: Test wallet keypair not found at $TEST_WALLET_KEYPAIR"
    exit 1
fi

# 根据网络自动设置 RPC URL
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
if [ -n "$TEST_WALLET" ]; then
    echo "Test Wallet: $TEST_WALLET"
fi
echo "============================================"
echo ""

# ===== Step 1: Build Epoch Snapshot on ICP =====

echo "[Step 1/4] Building epoch snapshot on ICP backend..."
if ! dfx_call canister call "$BACKEND_CANISTER_ID" build_epoch_snapshot "($EPOCH : nat64)"; then
    echo "❌ Error: Failed to build epoch snapshot"
    exit 1
fi
echo "✅ Epoch snapshot built successfully"
echo ""

# ===== Step 2: Get Epoch Metadata =====

echo "[Step 2/4] Fetching epoch metadata..."
META_RESULT=$(dfx_call canister call "$BACKEND_CANISTER_ID" get_epoch_meta "($EPOCH : nat64)" 2>&1)

if echo "$META_RESULT" | grep -qi "null"; then
    echo "❌ Error: Epoch metadata not found"
    exit 1
fi

export TEMP_META_RESULT="$META_RESULT"
PARSED_JSON=$(python3 << 'PYEOF'
import sys, re, os, json
result = os.environ.get("TEMP_META_RESULT", "")
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
                elif blob_str[i+1] == '\\': hex_val += '5c'; i += 2
                elif blob_str[i+1] == '"': hex_val += '22'; i += 2
                elif blob_str[i+1] == 'n': hex_val += '0a'; i += 2
                elif blob_str[i+1] == 'r': hex_val += '0d'; i += 2
                elif blob_str[i+1] == 't': hex_val += '09'; i += 2
                else: hex_val += '5c'; i += 1
            else: hex_val += '5c'; i += 1
        else: hex_val += format(ord(blob_str[i]), '02x'); i += 1
    if len(hex_val) == 64: root_hex = hex_val
if not root_hex:
    root_match = re.search(r'root\s*=\s*vec\s*\{([^}]+)\}', result)
    if root_match:
        bytes_list = re.findall(r'0x([0-9a-fA-F]{2})', root_match.group(1))
        if len(bytes_list) == 32: root_hex = ''.join(bytes_list)
leaves_count = 0
count_match = re.search(r'leaves_count\s*=\s*([0-9_]+)', result)
if count_match: leaves_count = int(count_match.group(1).replace('_', ''))
print(json.dumps({"root": root_hex, "count": leaves_count}))
PYEOF
)
unset TEMP_META_RESULT

ROOT_HEX=$(echo "$PARSED_JSON" | python3 -c "import sys, json; print(json.load(sys.stdin)['root'])" | tail -n 1)
LEAVES_COUNT=$(echo "$PARSED_JSON" | python3 -c "import sys, json; print(json.load(sys.stdin)['count'])" | tail -n 1)

if [ -z "$ROOT_HEX" ] || [ "$LEAVES_COUNT" = "0" ]; then
    echo "❌ Error: Failed to parse metadata (Root: $ROOT_HEX, Count: $LEAVES_COUNT)"
    exit 1
fi
echo "✅ Parsed epoch metadata: Root=$ROOT_HEX, Count=$LEAVES_COUNT"
echo ""

# ===== Step 3: Get Claim Ticket (Optional) =====

if [ -n "$TEST_WALLET" ]; then
    echo "[Step 2.5/4] Fetching claim ticket for $TEST_WALLET..."
    TICKET_RESULT=$(dfx_call canister call "$BACKEND_CANISTER_ID" get_claim_ticket "(\"$TEST_WALLET\")" 2>&1)
    
    if echo "$TICKET_RESULT" | grep -qi "err"; then
        echo "⚠️  Warning: Failed to get claim ticket: $TICKET_RESULT"
        echo "Will skip claim verification."
        TEST_WALLET=""
    else
        # Parse ticket
        export TEMP_TICKET_RESULT="$TICKET_RESULT"
        TICKET_JSON=$(python3 << 'PYEOF'
import sys, re, os, json
result = os.environ.get("TEMP_TICKET_RESULT", "")
index_match = re.search(r'index\s*=\s*([0-9_]+)', result)
amount_match = re.search(r'amount\s*=\s*([0-9_]+)', result)
proof_match = re.search(r'proof\s*=\s*vec\s*\{([^}]+)\}', result)

index = int(index_match.group(1).replace('_', '')) if index_match else 0
amount = int(amount_match.group(1).replace('_', '')) if amount_match else 0
proof_hex_list = []
if proof_match:
    blobs = re.findall(r'blob\s*"([^"]+)"', proof_match.group(1))
    for blob_str in blobs:
        hex_val = ""
        i = 0
        while i < len(blob_str):
            if blob_str[i] == '\\':
                if i + 1 < len(blob_str):
                    if re.match(r'[0-9a-fA-F]{2}', blob_str[i+1:i+3]):
                        hex_val += blob_str[i+1:i+3].lower()
                        i += 3
                    elif blob_str[i+1] == '\\': hex_val += '5c'; i += 2
                    else: hex_val += '5c'; i += 1
                else: hex_val += '5c'; i += 1
            else: hex_val += format(ord(blob_str[i]), '02x'); i += 1
        proof_hex_list.append(hex_val)

print(json.dumps({"index": index, "amount": amount, "proof": proof_hex_list}))
PYEOF
)
        unset TEMP_TICKET_RESULT
        
        TICKET_INDEX=$(echo "$TICKET_JSON" | python3 -c "import sys, json; print(json.load(sys.stdin)['index'])")
        TICKET_AMOUNT=$(echo "$TICKET_JSON" | python3 -c "import sys, json; print(json.load(sys.stdin)['amount'])")
        TICKET_PROOF=$(echo "$TICKET_JSON" | python3 -c "import sys, json; print(','.join(json.load(sys.stdin)['proof']))")
        
        echo "✅ Got claim ticket: Index=$TICKET_INDEX, Amount=$TICKET_AMOUNT"
    fi
    echo ""
fi

# ===== Step 4: Submit to Solana =====

echo "[Step 3/4] Submitting root to Solana..."
solana config set --url "$SOLANA_RPC_URL"

# 切换到 pmug-distributor 目录
cd "$PMUG_DISTRIBUTOR_DIR"

SUBMIT_SCRIPT="$PMUG_DISTRIBUTOR_DIR/submit_epoch_${EPOCH}_$$.js"

cat > "$SUBMIT_SCRIPT" << 'NODEEOF'
const anchor = require('@coral-xyz/anchor');
const { PublicKey, SystemProgram, Keypair } = require('@solana/web3.js');
const { getAssociatedTokenAddressSync, createAssociatedTokenAccountInstruction } = require('@solana/spl-token');
const fs = require('fs');

async function main() {
  const PROGRAM_ID = new PublicKey(process.env.DISTRIBUTOR_PROGRAM_ID);
  const RPC_URL = process.env.SOLANA_RPC_URL;
  const ADMIN_KEYPAIR_PATH = process.env.ADMIN_KEYPAIR_PATH;
  const IDL_PATH = process.env.IDL_PATH;
  const EPOCH = BigInt(process.env.EPOCH);
  const ROOT_HEX = process.env.ROOT_HEX;
  const LEAVES_COUNT = parseInt(process.env.LEAVES_COUNT);
  
  const connection = new anchor.web3.Connection(RPC_URL, 'confirmed');
  const adminKeypair = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(ADMIN_KEYPAIR_PATH, 'utf-8'))));
  const provider = new anchor.AnchorProvider(connection, new anchor.Wallet(adminKeypair), { commitment: 'confirmed' });
  const idl = JSON.parse(fs.readFileSync(IDL_PATH, 'utf-8'));
  const program = new anchor.Program(idl, provider);
  
  const [distributor] = PublicKey.findProgramAddressSync([Buffer.from("distributor")], program.programId);
  const epochBuf = Buffer.allocUnsafe(8);
  epochBuf.writeBigUInt64LE(EPOCH);
  const [epochData] = PublicKey.findProgramAddressSync([Buffer.from("epoch"), distributor.toBuffer(), epochBuf], program.programId);
  
  const root = Array.from(Buffer.from(ROOT_HEX, 'hex'));
  
  console.log(`🚀 Submitting Root for Epoch ${EPOCH}...`);
  try {
    // 检查账户是否已存在
    const accountInfo = await connection.getAccountInfo(epochData);
    if (accountInfo) {
      console.log("⚠️  Epoch root already exists on-chain. Skipping submission.");
    } else {
      const tx = await program.methods.updateEpochRoot(new anchor.BN(EPOCH.toString()), root, LEAVES_COUNT)
        .accounts({ distributor, admin: adminKeypair.publicKey, epochData, systemProgram: SystemProgram.id }).rpc();
      console.log(`✅ Root submitted: ${tx}`);
    }
  } catch (e) {
    if (e.message.includes("already exists") || e.message.includes("0x1771") || e.message.includes("already in use")) {
      console.log("⚠️  Epoch root already exists on-chain.");
    } else { throw e; }
  }

  // Claim Verification
  if (process.env.TEST_WALLET && process.env.TICKET_PROOF) {
    console.log('\n[Step 4/4] Verifying Claim...');
    const claimerKeypair = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(process.env.TEST_WALLET_KEYPAIR, 'utf-8'))));
    const claimer = claimerKeypair.publicKey;
    const index = parseInt(process.env.TICKET_INDEX);
    const amount = new anchor.BN(process.env.TICKET_AMOUNT);
    const proof = process.env.TICKET_PROOF.split(',').map(p => Array.from(Buffer.from(p, 'hex')));
    
    const distributorAccount = await program.account.distributor.fetch(distributor);
    const mint = distributorAccount.tokenMint;
    const vault = distributorAccount.vault;
    const vaultAuthority = distributorAccount.vaultAuthority;
    
    const claimerAta = getAssociatedTokenAddressSync(mint, claimer);
    const indexBuf = Buffer.alloc(4);
    indexBuf.writeUInt32LE(index);
    const [claimStatus] = PublicKey.findProgramAddressSync([Buffer.from("claim"), distributor.toBuffer(), indexBuf], program.programId);
    
    // Check if ATA exists
    const ataInfo = await connection.getAccountInfo(claimerAta);
    const instructions = [];
    if (!ataInfo) {
      console.log(`Creating ATA for ${claimer.toString()}...`);
      instructions.push(createAssociatedTokenAccountInstruction(adminKeypair.publicKey, claimerAta, claimer, mint));
    }

    try {
      const tx = await program.methods.claim(new anchor.BN(EPOCH.toString()), index, amount, proof)
        .accounts({
          distributor, epochData, claimStatus, claimer, claimerTokenAccount: claimerAta,
          vault, vaultAuthority, tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID, systemProgram: SystemProgram.programId
        })
        .preInstructions(instructions)
        .signers([claimerKeypair])
        .rpc();
      console.log(`✅ Claim verification successful: ${tx}`);
    } catch (e) {
      if (e.message.includes("AlreadyClaimed") || e.message.includes("0x1774")) {
        console.log("✅ Already claimed (Verification passed)");
      } else {
        console.error("❌ Claim verification failed:");
        console.error(e);
        process.exit(1);
      }
    }
  }
}

main().then(() => process.exit(0)).catch(e => { console.error(e); process.exit(1); });
NODEEOF

export DISTRIBUTOR_PROGRAM_ID SOLANA_RPC_URL ADMIN_KEYPAIR_PATH IDL_PATH EPOCH ROOT_HEX LEAVES_COUNT SOLANA_NETWORK
export TEST_WALLET TEST_WALLET_KEYPAIR TICKET_INDEX TICKET_AMOUNT TICKET_PROOF

if ! node "$SUBMIT_SCRIPT"; then
    rm -f "$SUBMIT_SCRIPT"
    exit 1
fi

rm -f "$SUBMIT_SCRIPT"

echo ""
echo "============================================"
echo "✅ Epoch Submission & Verification Complete"
echo "============================================"
echo "Epoch: $EPOCH"
echo "Status: SUCCESS"
echo "============================================"
