#!/bin/bash
# Full Flow Test Script for Merkle Distributor
# 
# 此脚本自动完成以下全流程：
# 1. 生成/加载 5 个测试钱包
# 2. 为钱包在 Solana Localnet 充值 (Airdrop)
# 3. 在 ICP 后端为这 5 个钱包 Mock 奖励数据
# 4. 提交 Merkle Root 到 Solana
# 5. 逐个验证钱包的 Claim 领取功能

set -e

# 获取项目根目录
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# 切换到项目根目录，确保 dfx 命令可以正确执行
cd "$PROJECT_ROOT" || { echo "❌ 无法切换到项目根目录: $PROJECT_ROOT"; exit 1; }

WALLETS_DIR="$PROJECT_ROOT/pmug-distributor/test-wallets"
NUM_WALLETS=5

echo "=================================================="
echo "🚀 Starting Full Flow Test (Localnet)"
echo "=================================================="

# 1. 准备钱包
mkdir -p "$WALLETS_DIR"
WALLET_ADDRS=()
WALLET_FILES=()

echo "[Step 1/5] Preparing $NUM_WALLETS test wallets..."
for i in $(seq 1 $NUM_WALLETS); do
    FILE="$WALLETS_DIR/wallet_$i.json"
    if [ ! -f "$FILE" ]; then
        solana-keygen new --no-passphrase -o "$FILE" > /dev/null
    fi
    ADDR=$(solana address -k "$FILE")
    WALLET_ADDRS+=("$ADDR")
    WALLET_FILES+=("$FILE")
    echo "  Wallet $i: $ADDR"
done
echo ""

# 2. Localnet Airdrop
echo "[Step 2/5] Funding wallets on Solana Localnet..."
for addr in "${WALLET_ADDRS[@]}"; do
    echo "  Airdropping to $addr..."
    solana airdrop 2 "$addr" --url http://localhost:8899 > /dev/null 2>&1 || true
done
echo "✅ Wallets funded."
echo ""

# 3. ICP Mock Rewards
echo "[Step 3/5] Mocking rewards on ICP backend..."
MOCK_CMD=("$SCRIPT_DIR/mock_claimable_rewards.sh")
for addr in "${WALLET_ADDRS[@]}"; do
    MOCK_CMD+=("-w" "$addr")
done

# 运行 mock 并获取输出，提取 Epoch 编号
MOCK_OUTPUT=$("${MOCK_CMD[@]}")
echo "$MOCK_OUTPUT" | grep -E "处理钱包|生成 Epoch|构建快照成功"

EPOCH=$(echo "$MOCK_OUTPUT" | grep "自动选择 Epoch:" | awk '{print $NF}')
if [ -z "$EPOCH" ]; then
    # 尝试从另一种格式提取
    EPOCH=$(echo "$MOCK_OUTPUT" | grep "正在为 Epoch" | awk '{print $4}')
fi

if [ -z "$EPOCH" ]; then
    echo "❌ Error: Could not determine Epoch number from mock output."
    exit 1
fi
echo "✅ Mock data created for Epoch: $EPOCH"
echo ""

# 4 & 5. Submit Root & Verify Claims
echo "[Step 4 & 5/5] Submitting root and verifying claims..."
SUBMIT_SCRIPT="$SCRIPT_DIR/epoch_submit_claim.sh"

for i in $(seq 0 $((NUM_WALLETS - 1))); do
    ADDR="${WALLET_ADDRS[$i]}"
    FILE="${WALLET_FILES[$i]}"
    
    echo "--------------------------------------------------"
    echo "Verifying Wallet $((i+1)): $ADDR"
    
    # 第一次运行会提交 Root，后续运行会检查到 Root 已存在并仅验证 Claim
    if ! "$SUBMIT_SCRIPT" "$EPOCH" "$ADDR" "$FILE"; then
        echo "❌ Claim verification failed for wallet $ADDR"
        exit 1
    fi
done

echo ""
echo "=================================================="
echo "🎉 Full Flow Test Completed Successfully!"
echo "=================================================="
echo "Summary:"
echo "  - Wallets Tested: $NUM_WALLETS"
echo "  - Epoch: $EPOCH"
echo "  - Status: All claims verified on Solana Localnet"
echo "=================================================="
