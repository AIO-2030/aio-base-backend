#!/bin/bash
# Mock Claimable Rewards Script
# 
# 此脚本用于在 ICP 后端 canister 中 mock claimable rewards 数据
# 通过创建用户任务并完成任务来生成可领取的奖励
# 
# 使用方法：
#   ./mock_claimable_rewards.sh [options]
# 
# 选项：
#   -w, --wallet WALLET      Solana 钱包地址（Base58 格式）
#   -n, --count COUNT        要 mock 的用户数量（默认：5）
#   -t, --task TASKID        要完成的任务 ID（默认：所有任务）
#   -a, --amount AMOUNT      奖励金额（PMUG 最小单位，默认：使用任务合约中的金额）
#   -e, --epoch EPOCH        目标 epoch（可选，用于测试特定 epoch）
#   -h, --help               显示帮助信息
# 
# 示例：
#   # Mock 5 个用户的奖励
#   ./mock_claimable_rewards.sh -n 5
# 
#   # Mock 特定钱包的奖励
#   ./mock_claimable_rewards.sh -w "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU"
# 
#   # Mock 特定任务的奖励
#   ./mock_claimable_rewards.sh -t "register_device" -n 3

# 不使用 set -e，手动处理错误
# set -e 会导致 Python 脚本返回非零退出码时脚本立即退出
# set -e

# ===== Configuration =====

# 获取项目根目录
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# 直接切换到项目根目录
cd "$PROJECT_ROOT" || { echo "❌ 无法切换到项目根目录: $PROJECT_ROOT"; exit 1; }

# ICP Canister ID
BACKEND_CANISTER_ID="uxrrr-q7777-77774-qaaaq-cai"

# 默认配置
DEFAULT_USER_COUNT=5
DEFAULT_TASK_IDS=("register_device" "ai_subscription" "voice_clone")

# 定义 dfx 调用函数
dfx_call() {
    dfx "$@"
}

# ===== Helper Functions =====

show_help() {
    cat << EOF
Mock Claimable Rewards Script

用法: $0 [选项]

选项:
  -w, --wallet WALLET      Solana 钱包地址（Base58 格式）
  -n, --count COUNT        要 mock 的用户数量（默认：$DEFAULT_USER_COUNT）
  -t, --task TASKID        要完成的任务 ID（可多次指定，默认：所有任务）
  -a, --amount AMOUNT      奖励金额（PMUG 最小单位，覆盖任务合约中的金额）
  -e, --epoch EPOCH        目标 epoch（可选）
  -h, --help               显示帮助信息

示例:
  # Mock 5 个用户的奖励
  $0 -n 5

  # Mock 特定钱包的奖励
  $0 -w "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU"

  # Mock 特定任务的奖励
  $0 -t "register_device" -n 3

  # Mock 多个任务的奖励
  $0 -t "register_device" -t "ai_subscription" -n 3
EOF
}

# 生成随机 Solana 钱包地址（Base58 格式，32 字节）
generate_random_wallet() {
    # 生成 32 字节的随机数据并转换为 Base58
    # 注意：这是简化的 mock 地址，不是真实的 Solana 密钥对
    python3 << PYEOF
import sys
import secrets

try:
    import base58
except ImportError:
    # 如果没有 base58 库，使用简单的 Base58 编码实现
    BASE58_ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
    
    def base58_encode(data):
        num = int.from_bytes(data, 'big')
        encoded = ''
        while num > 0:
            encoded = BASE58_ALPHABET[num % 58] + encoded
            num //= 58
        # 处理前导零
        for byte in data:
            if byte == 0:
                encoded = '1' + encoded
            else:
                break
        return encoded
    
    # 生成 32 字节随机数据
    random_bytes = secrets.token_bytes(32)
    wallet = base58_encode(random_bytes)
    print(wallet)
else:
    # 生成 32 字节随机数据
    random_bytes = secrets.token_bytes(32)
    # 转换为 Base58（Solana 地址格式）
    wallet = base58.b58encode(random_bytes).decode('utf-8')
    print(wallet)
PYEOF
}

# 验证钱包地址格式（Base58）
validate_wallet() {
    local wallet="$1"
    export TEMP_WALLET="$wallet"
    python3 << 'PYEOF'
import sys
import os

wallet = os.environ.get("TEMP_WALLET", "")

try:
    import base58
    decoded = base58.b58decode(wallet)
    if len(decoded) == 32:
        print("valid")
    else:
        print("invalid")
        sys.exit(1)
except ImportError:
    # 如果没有 base58 库，进行基本格式验证
    BASE58_ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
    if wallet and all(c in BASE58_ALPHABET for c in wallet) and 32 <= len(wallet) <= 44:
        print("valid")
    else:
        print("invalid")
        sys.exit(1)
except:
    print("invalid")
    sys.exit(1)
PYEOF
    unset TEMP_WALLET
}

# 获取当前时间戳（纳秒）
# ICP 使用纳秒时间戳
get_timestamp() {
    # 获取秒级时间戳并转换为纳秒
    python3 << PYEOF
import time
timestamp_ns = int(time.time() * 1_000_000_000)
print(timestamp_ns)
PYEOF
}

# ===== Parse Arguments =====

WALLET=""
USER_COUNT=$DEFAULT_USER_COUNT
TASK_IDS=()
CUSTOM_AMOUNT=""
EPOCH=""

while [[ $# -gt 0 ]]; do
    case $1 in
        -w|--wallet)
            WALLETS+=("$2")
            shift 2
            ;;
        -n|--count)
            USER_COUNT="$2"
            shift 2
            ;;
        -t|--task)
            TASK_IDS+=("$2")
            shift 2
            ;;
        -a|--amount)
            CUSTOM_AMOUNT="$2"
            shift 2
            ;;
        -e|--epoch)
            EPOCH="$2"
            shift 2
            ;;
        -h|--help)
            show_help
            exit 0
            ;;
        *)
            echo "❌ 未知选项: $1"
            echo "使用 -h 或 --help 查看帮助"
            exit 1
            ;;
    esac
done

# ===== Validate Configuration =====

if [ "$BACKEND_CANISTER_ID" = "your-backend-canister-id" ]; then
    echo "❌ Error: Please set BACKEND_CANISTER_ID in the script"
    exit 1
fi

# 如果没有指定任务，使用默认任务列表
if [ ${#TASK_IDS[@]} -eq 0 ]; then
    TASK_IDS=("${DEFAULT_TASK_IDS[@]}")
fi

# 如果没有指定钱包，生成随机钱包列表
if [ ${#WALLETS[@]} -gt 0 ]; then
    # 验证钱包格式
    for w in "${WALLETS[@]}"; do
        if ! validate_wallet "$w" | grep -q "valid"; then
            echo "❌ Error: Invalid wallet format: $w"
            echo "Wallet must be a valid Base58-encoded 32-byte Solana address"
            exit 1
        fi
    done
    USER_COUNT=${#WALLETS[@]}
else
    echo "生成 $USER_COUNT 个随机钱包地址..."
    for ((i=0; i<USER_COUNT; i++)); do
        wallet=$(generate_random_wallet)
        WALLETS+=("$wallet")
        echo "  [$((i+1))] $wallet"
    done
fi

echo ""
echo "============================================"
echo "Mock Claimable Rewards Script"
echo "============================================"
echo "Backend Canister: $BACKEND_CANISTER_ID"
echo "Wallet Count: ${#WALLETS[@]}"
echo "Tasks: ${TASK_IDS[*]}"
if [ -n "$CUSTOM_AMOUNT" ]; then
    echo "Custom Amount: $CUSTOM_AMOUNT PMUG (smallest unit)"
fi
if [ -n "$EPOCH" ]; then
    echo "Target Epoch: $EPOCH"
fi
echo "============================================"
echo ""

# ===== Step 1: Initialize or Get Task Contract =====

echo "[Step 1/4] 检查并初始化任务合约..."
echo ""

# 使用 dfx_call 确保在项目根目录执行
TASK_CONTRACT=$(dfx_call canister call "$BACKEND_CANISTER_ID" get_task_contract 2>&1)

# 调试：如果设置了 DEBUG 环境变量，显示原始输出
if [ "$DEBUG" = "1" ]; then
    echo "调试: get_task_contract 原始输出:"
    echo "$TASK_CONTRACT"
    echo "---"
fi

# 检查任务合约是否为空
TASK_COUNT=$(echo "$TASK_CONTRACT" | python3 << PYEOF
import sys
import re

result = sys.stdin.read()

# 计算任务数量（查找 record { ... } 的数量）
# Candid 格式可能是：
#   vec { record { ... }; record { ... }; ... }
#   或
#   (
#     vec {
#       record { ... };
#       record { ... };
#     },
#   )

# 方法1: 查找所有 record { 的数量（最可靠）
record_matches = re.findall(r'record\s*\{', result)
if record_matches:
    print(len(record_matches))
    sys.exit(0)

# 方法2: 查找 taskid = "..." 的数量
taskid_matches = re.findall(r'taskid\s*=\s*"([^"]+)"', result)
if taskid_matches:
    print(len(taskid_matches))
    sys.exit(0)

# 方法3: 查找 reward = ... 的数量（每个任务都有 reward）
reward_matches = re.findall(r'reward\s*=\s*\d+', result)
if reward_matches:
    print(len(reward_matches))
    sys.exit(0)

# 如果都没找到，返回0
print(0)
PYEOF
) || TASK_COUNT=0

# 检查是否为空（考虑多种格式）
# 如果 TASK_COUNT > 0，说明任务合约不为空
IS_EMPTY=false
if [ "$TASK_COUNT" = "0" ] || [ -z "$TASK_COUNT" ]; then
    # TASK_COUNT 为 0 或空，进一步检查是否是真正的空 vec
    IS_EMPTY_CHECK=$(echo "$TASK_CONTRACT" | python3 << PYEOF
import sys
import re

result = sys.stdin.read()

# 检查是否是空的 vec {} 或 vec { } 或 vec {, }
# 但如果有 record 或 taskid，说明不是空的
if re.search(r'record\s*\{', result) or re.findall(r'taskid\s*=\s*"', result):
    print("false")
    sys.exit(0)

# 检查是否是空的 vec
if re.search(r'vec\s*\{\s*\}', result) or re.search(r'vec\s*\{\s*,', result):
    print("true")
    sys.exit(0)

# 默认认为不为空（可能是格式问题导致检测失败）
print("false")
PYEOF
)
    if [ "$IS_EMPTY_CHECK" = "true" ] || [ "$IS_EMPTY_CHECK" = "True" ]; then
        IS_EMPTY=true
    fi
fi

# 调试输出（如果需要）
if [ "$DEBUG" = "1" ]; then
    echo "调试: TASK_COUNT=$TASK_COUNT"
    echo "调试: IS_EMPTY=$IS_EMPTY"
    echo "调试: TASK_CONTRACT 前300字符:"
    echo "$TASK_CONTRACT" | head -c 300
    echo ""
fi

if [ "$IS_EMPTY" = "true" ]; then
    echo "⚠️  任务合约为空"
    echo ""
    echo "提示: 任务合约应该在 build-aichat.sh 启动时自动初始化"
    echo "如果任务合约未初始化，请运行:"
    echo "  dfx canister call $BACKEND_CANISTER_ID init_task_contract \"(...)\""
    echo ""
    echo "或者重新运行 build-aichat.sh 来初始化任务合约"
    echo ""
    read -p "是否现在初始化任务合约? (y/N): " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo "正在初始化任务合约..."
        # 初始化任务合约（与 build-aichat.sh 中的配置一致）
        INIT_RESULT=$(dfx_call canister call "$BACKEND_CANISTER_ID" init_task_contract \
            "(vec {
                record { taskid = \"invite_20_friends\"; reward = 50_000_000 : nat64; payfor = null : opt text; };
                record { taskid = \"register_device\"; reward = 50_000_000 : nat64; payfor = null : opt text; };
                record { taskid = \"ai_subscription\"; reward = 100_000_000 : nat64; payfor = opt \"ai_subscription\" : opt text; };
                record { taskid = \"voice_clone\"; reward = 150_000_000 : nat64; payfor = null : opt text; };
            })" 2>&1)
        
        if echo "$INIT_RESULT" | grep -q "Ok"; then
            echo "✅ 任务合约初始化成功"
            # 重新获取任务合约
            TASK_CONTRACT=$(dfx_call canister call "$BACKEND_CANISTER_ID" get_task_contract 2>&1)
            TASK_COUNT=$(echo "$TASK_CONTRACT" | python3 << PYEOF
import sys
import re
result = sys.stdin.read()
matches = re.findall(r'record \{', result)
print(len(matches))
PYEOF
)
        else
            echo "❌ Error: Failed to initialize task contract"
            echo "$INIT_RESULT"
            exit 1
        fi
    else
        echo "跳过任务合约初始化，继续执行..."
    fi
else
    echo "✅ 任务合约已存在（包含 $TASK_COUNT 个任务）"
    
    # 调试：显示任务合约内容（前500字符）
    if [ "$DEBUG" = "1" ]; then
        echo ""
        echo "任务合约内容（调试）:"
        echo "$TASK_CONTRACT" | head -c 500
        echo "..."
        echo ""
    fi
fi

echo ""

# 解析任务合约以获取任务奖励金额
get_task_reward() {
    local taskid="$1"
    export TEMP_TASK_CONTRACT="$TASK_CONTRACT"
    export TEMP_TASKID="$taskid"
    python3 << 'PYEOF'
import sys
import re
import os

task_contract = os.environ.get("TEMP_TASK_CONTRACT", "")
taskid = os.environ.get("TEMP_TASKID", "")

# 1. 提取所有的 record 块
records = re.findall(r'record\s*\{(.*?)\}', task_contract, re.DOTALL)

for record in records:
    # 2. 检查当前 record 是否包含目标 taskid
    if re.search(rf'taskid\s*=\s*"{re.escape(taskid)}"', record):
        # 3. 从该 record 中提取 reward
        reward_match = re.search(r'reward\s*=\s*([\d_]+)', record)
        if reward_match:
            print(reward_match.group(1).replace('_', ''))
            sys.exit(0)
sys.exit(1)
PYEOF
    unset TEMP_TASK_CONTRACT
    unset TEMP_TASKID
}

# 列出所有可用的任务 ID
list_available_tasks() {
    export TEMP_TASK_CONTRACT="$TASK_CONTRACT"
    python3 << 'PYEOF'
import sys
import re
import os

task_contract = os.environ.get("TEMP_TASK_CONTRACT", "")
# 提取所有 taskid
taskids = re.findall(r'taskid\s*=\s*"([^"]+)"', task_contract)
if taskids:
    print("\n可用任务列表:")
    for tid in taskids:
        print(f"  - {tid}")
else:
    print("\n⚠️  未找到任何任务")
PYEOF
    unset TEMP_TASK_CONTRACT
}

# 显示可用任务
list_available_tasks
echo ""

# ===== Step 2: Initialize User Tasks and Complete Tasks =====

echo "[Step 2/4] 创建用户任务并完成任务..."
echo ""

SUCCESS_COUNT=0
FAIL_COUNT=0
TIMESTAMP=$(get_timestamp)

for wallet in "${WALLETS[@]}"; do
    echo "处理钱包: $wallet"
    
    # 初始化用户任务（确保在项目根目录）
    INIT_RESULT=$(dfx_call canister call "$BACKEND_CANISTER_ID" get_or_init_user_tasks "(\"$wallet\")" 2>&1)
    
    if echo "$INIT_RESULT" | grep -q "Err"; then
        echo "  ⚠️  警告: 初始化用户任务时出现错误"
        echo "$INIT_RESULT" | grep "Err" | sed 's/^/    /'
    else
        # 检查是否成功获取到任务列表
        # Candid 格式可能是：tasks = vec { record { ... }; record { ... }; ... }
        TASK_COUNT_IN_USER=$(echo "$INIT_RESULT" | python3 << PYEOF
import sys
import re

try:
    result = sys.stdin.read()

    # 尝试多种方式查找任务数量
    # 1. 查找 tasks = vec { ... } 中的 record 数量
    # 2. 查找 taskid = "..." 的数量
    # 3. 查找 status = ... 的数量（每个任务都有 status）

    # 方法1: 查找 tasks = vec { 之后的 record
    tasks_match = re.search(r'tasks\s*=\s*vec\s*\{([^}]*)\}', result, re.DOTALL)
    if tasks_match:
        tasks_content = tasks_match.group(1)
        # 计算 record { 的数量
        record_count = len(re.findall(r'record\s*\{', tasks_content))
        if record_count > 0:
            print(record_count)
            sys.exit(0)

    # 方法2: 查找所有 taskid
    taskid_matches = re.findall(r'taskid\s*=\s*"([^"]+)"', result)
    if taskid_matches:
        print(len(taskid_matches))
        sys.exit(0)

    # 方法3: 查找所有 status（每个任务都有 status）
    status_matches = re.findall(r'status\s*=\s*(\w+)', result)
    if status_matches:
        print(len(status_matches))
        sys.exit(0)

    # 如果都没找到，返回0（这不是错误）
    print(0)
    sys.exit(0)
except Exception as e:
    # 即使出错也返回0，避免脚本退出
    print(0)
    sys.exit(0)
PYEOF
) || TASK_COUNT_IN_USER=0
        
        if [ "$TASK_COUNT_IN_USER" -gt 0 ]; then
            echo "  ✅ 用户任务已初始化（包含 $TASK_COUNT_IN_USER 个任务）"
        else
            # 即使没找到任务数量，只要没有错误就认为成功
            if ! echo "$INIT_RESULT" | grep -qi "error\|failed"; then
                echo "  ✅ 用户任务已初始化"
            else
                echo "  ⚠️  警告: 用户任务初始化可能有问题"
                # 显示部分结果用于调试
                echo "$INIT_RESULT" | head -20 | sed 's/^/    /'
            fi
        fi
    fi
    
    # 完成每个任务（确保这部分一定会执行）
    echo "  开始完成任务..."
    for taskid in "${TASK_IDS[@]}"; do
        # 获取任务奖励金额
        if [ -n "$CUSTOM_AMOUNT" ]; then
            reward_amount="$CUSTOM_AMOUNT"
        else
            reward_amount=$(get_task_reward "$taskid" 2>/dev/null || echo "")
            if [ -z "$reward_amount" ] || [ "$reward_amount" = "0" ]; then
                # 如果解析失败，尝试使用默认值
                case "$taskid" in
                    register_device)
                        reward_amount=50000000
                        echo "  ⚠️  警告: 任务 '$taskid' 未在合约中找到，使用默认奖励: $reward_amount"
                        ;;
                    ai_subscription)
                        reward_amount=100000000
                        echo "  ⚠️  警告: 任务 '$taskid' 未在合约中找到，使用默认奖励: $reward_amount"
                        ;;
                    voice_clone)
                        reward_amount=150000000
                        echo "  ⚠️  警告: 任务 '$taskid' 未在合约中找到，使用默认奖励: $reward_amount"
                        ;;
                    invite_20_friends)
                        reward_amount=200000000
                        echo "  ⚠️  警告: 任务 '$taskid' 未在合约中找到，使用默认奖励: $reward_amount"
                        ;;
                    *)
                        echo "  ⚠️  警告: 任务 '$taskid' 未找到且无默认奖励，跳过"
                        echo "    提示: 请使用 -a 选项指定自定义奖励金额"
                        continue
                        ;;
                esac
            fi
        fi
        
        echo "  - 完成任务: $taskid (奖励: $reward_amount PMUG 最小单位)"
        
        # 调用 complete_task
        # 注意：evidence 参数是可选的，这里传 null
        # Candid 格式：complete_task(wallet: text, taskid: text, evidence: opt text, ts: nat64)
        COMPLETE_RESULT=$(dfx_call canister call "$BACKEND_CANISTER_ID" complete_task \
            "(\"$wallet\", \"$taskid\", null : opt text, $TIMESTAMP : nat64)" 2>&1)
        
        if echo "$COMPLETE_RESULT" | grep -q "Ok"; then
            echo "    ✅ 任务完成成功"
            SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
        elif echo "$COMPLETE_RESULT" | grep -qi "already completed\|already exists"; then
            echo "    ⚠️  任务已完成（跳过）"
        elif echo "$COMPLETE_RESULT" | grep -q "Err"; then
            echo "    ❌ 任务完成失败:"
            echo "$COMPLETE_RESULT" | sed 's/^/      /'
            FAIL_COUNT=$((FAIL_COUNT + 1))
        else
            # 检查是否包含错误信息
            if echo "$COMPLETE_RESULT" | grep -qi "error\|failed\|not found"; then
                echo "    ❌ 任务完成失败:"
                echo "$COMPLETE_RESULT" | sed 's/^/      /'
                FAIL_COUNT=$((FAIL_COUNT + 1))
            else
                # 可能是成功但没有明确的 Ok 标记
                echo "    ⚠️  未知结果，请检查:"
                echo "$COMPLETE_RESULT" | sed 's/^/      /'
            fi
        fi
    done
    
    echo ""
done

# ===== Step 3: Build Epoch Snapshot =====

echo "[Step 3/4] 生成 Epoch 快照以激活奖励..."
echo ""

# 如果没有指定 Epoch，自动寻找下一个可用的 Epoch
if [ -z "$EPOCH" ]; then
    echo "未指定 Epoch，正在查询最新状态..."
    # 尝试获取所有 Epoch 并找出最大的
    ALL_EPOCHS=$(dfx_call canister call "$BACKEND_CANISTER_ID" list_all_epochs 2>&1)
    
    # 解析最大 Epoch ID
    MAX_EPOCH=$(echo "$ALL_EPOCHS" | python3 << PYEOF
import sys
import re

result = sys.stdin.read()
# 查找 epoch = ... : nat64
epochs = re.findall(r'epoch\s*=\s*(\d+)\s*:', result)
if epochs:
    print(max(int(e) for e in epochs))
else:
    print(0)
PYEOF
)
    EPOCH=$((MAX_EPOCH + 1))
    echo "自动选择 Epoch: $EPOCH"
fi

echo "正在为 Epoch $EPOCH 构建快照..."
SNAPSHOT_RESULT=$(dfx_call canister call "$BACKEND_CANISTER_ID" build_epoch_snapshot "($EPOCH : nat64)" 2>&1)

if echo "$SNAPSHOT_RESULT" | grep -q "Ok"; then
    echo "✅ Epoch $EPOCH 快照构建成功，奖励已进入 RewardPrepared 状态"
elif echo "$SNAPSHOT_RESULT" | grep -q "already exists"; then
    echo "⚠️  Epoch $EPOCH 快照已存在，跳过构建"
elif echo "$SNAPSHOT_RESULT" | grep -q "No claimable rewards found"; then
    echo "❌ 错误: 未找到可结算的奖励。请检查任务是否已成功完成。"
    echo "$SNAPSHOT_RESULT"
    # 不退出，继续验证
else
    echo "❌ 构建快照失败:"
    echo "$SNAPSHOT_RESULT"
    # 不退出，继续验证
fi

echo ""

# ===== Step 4: Verify Results =====

echo "[Step 4/4] 验证结果..."

# 验证每个钱包的任务状态
VERIFIED_COUNT=0
for wallet in "${WALLETS[@]}"; do
    USER_STATE=$(dfx_call canister call "$BACKEND_CANISTER_ID" get_or_init_user_tasks "(\"$wallet\")" 2>&1)
    
    # 检查状态是否为已完成、奖励已准备、凭证已签发或已领取
    # 只要不是 NotStarted 或 InProgress，就说明任务已经处理过
    if echo "$USER_STATE" | grep -qE "Completed|RewardPrepared|TicketIssued|Claimed"; then
        VERIFIED_COUNT=$((VERIFIED_COUNT + 1))
        
        # 提取总未领取金额
        # 使用 Python 提取以确保处理 Candid 复杂格式和下划线
        export TEMP_USER_STATE="$USER_STATE"
        TOTAL_UNCLAIMED=$(python3 << 'PYEOF'
import sys
import re
import os

result = os.environ.get("TEMP_USER_STATE", "")
# 查找 total_unclaimed = ... 之后的数字和下划线
match = re.search(r'total_unclaimed\s*=\s*([0-9_]+)', result)
if match:
    print(match.group(1).replace('_', ''), end='')
else:
    print("0", end='')
PYEOF
)
        unset TEMP_USER_STATE
        if [ -z "$TOTAL_UNCLAIMED" ]; then TOTAL_UNCLAIMED=0; fi
        
        # 确定具体状态用于显示
        STATUS="已处理"
        if echo "$USER_STATE" | grep -q "RewardPrepared"; then STATUS="奖励已快照 (RewardPrepared)"; fi
        if echo "$USER_STATE" | grep -q "TicketIssued"; then STATUS="凭证已签发 (TicketIssued)"; fi
        if echo "$USER_STATE" | grep -q "Claimed"; then STATUS="已领取 (Claimed)"; fi
        if echo "$USER_STATE" | grep -q "Completed"; then STATUS="已完成 (Completed)"; fi

        echo "✅ 钱包 $wallet: 状态 = $STATUS, 总未领取金额 = $TOTAL_UNCLAIMED PMUG"
    else
        echo "⚠️  钱包 $wallet: 未找到已完成或处理中的任务"
    fi
done

# ===== Summary =====

echo ""
echo "============================================"
echo "Mock 完成总结"
echo "============================================"
echo "处理的钱包数: ${#WALLETS[@]}"
echo "成功完成的任务数: $SUCCESS_COUNT"
echo "失败的任务数: $FAIL_COUNT"
echo "使用的 Epoch: $EPOCH"
echo "已验证的钱包数: $VERIFIED_COUNT"
echo "============================================"
echo ""

if [ $VERIFIED_COUNT -gt 0 ]; then
    echo "✅ Mock 数据创建并快照成功！"
    echo ""
    echo "📝 下一步:"
    echo "可以使用 epoch_submit.sh 将此 Epoch 提交到 Solana:"
    echo "   ./src/aio-base-backend/scripts/epoch_submit.sh $EPOCH"
    echo ""
else
    echo "⚠️  警告: 没有成功创建任何可领取的奖励"
    echo "请检查任务合约是否已初始化"
fi
echo ""
