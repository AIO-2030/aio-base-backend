#!/usr/bin/env python3
"""
Epoch Snapshot Submit Script (Python版本)

此脚本用于：
1. 在 ICP 后端生成 epoch 快照（Merkle tree）
2. 将 Merkle root 提交到 Solana 链上的 Distributor 合约

使用方法：
    python3 epoch_submit.py <epoch_number>

示例：
    python3 epoch_submit.py 1

依赖：
    pip install solders solana anchorpy
"""

import sys
import subprocess
import json
import base64
from pathlib import Path
from typing import List, Tuple

# ===== Configuration =====

# ICP Canister ID
BACKEND_CANISTER_ID = "your-backend-canister-id"  # 需要替换

# Solana Configuration
SOLANA_NETWORK = "mainnet-beta"  # 或 "devnet" / "testnet"
SOLANA_RPC_URL = "https://api.mainnet-beta.solana.com"

# Solana Program (Anchor)
DISTRIBUTOR_PROGRAM_ID = "your-distributor-program-id"
DISTRIBUTOR_ADDRESS = "your-distributor-address"

# Keypair
ADMIN_KEYPAIR_PATH = Path.home() / ".config" / "solana" / "id.json"

# ===== Helper Functions =====

def run_dfx_command(command: List[str]) -> Tuple[bool, str]:
    """运行 dfx 命令并返回结果"""
    try:
        result = subprocess.run(
            ["dfx"] + command,
            capture_output=True,
            text=True,
            check=True
        )
        return True, result.stdout
    except subprocess.CalledProcessError as e:
        return False, e.stderr


def parse_candid_blob(blob_str: str) -> List[int]:
    """解析 Candid blob 格式为字节数组"""
    # 示例输入: "vec { 1; 2; 3; ... }"
    # 提取数字
    import re
    numbers = re.findall(r'\d+', blob_str)
    return [int(n) for n in numbers]


def build_epoch_snapshot(epoch: int) -> bool:
    """在 ICP 后端构建 epoch 快照"""
    print(f"[Step 1/3] Building epoch snapshot on ICP backend...")
    print()
    
    success, output = run_dfx_command([
        "canister", "call",
        BACKEND_CANISTER_ID,
        "build_epoch_snapshot",
        f"({epoch} : nat64)"
    ])
    
    if not success:
        print(f"❌ Error: Failed to build epoch snapshot")
        print(output)
        return False
    
    if "Err" in output:
        print(f"❌ Error: {output}")
        return False
    
    print("✅ Epoch snapshot built successfully")
    print()
    return True


def get_epoch_metadata(epoch: int) -> Tuple[bool, dict]:
    """获取 epoch 元数据"""
    print(f"[Step 2/3] Fetching epoch metadata...")
    print()
    
    success, output = run_dfx_command([
        "canister", "call",
        BACKEND_CANISTER_ID,
        "get_epoch_meta",
        f"({epoch} : nat64)"
    ])
    
    if not success:
        print(f"❌ Error: Failed to fetch epoch metadata")
        print(output)
        return False, {}
    
    if "null" in output.lower():
        print(f"❌ Error: Epoch metadata not found")
        return False, {}
    
    # 解析输出
    # 这里需要根据实际的 Candid 输出格式解析
    # 简化版本：假设输出包含 root 和 leaves_count
    
    import re
    
    # 提取 root
    root_match = re.search(r'root = vec \{([^}]+)\}', output)
    if not root_match:
        print(f"❌ Error: Could not parse root from output")
        return False, {}
    
    root_str = root_match.group(1)
    root_bytes = parse_candid_blob(f"vec {{{root_str}}}")
    
    # 提取 leaves_count
    count_match = re.search(r'leaves_count = (\d+)', output)
    if not count_match:
        print(f"❌ Error: Could not parse leaves_count from output")
        return False, {}
    
    leaves_count = int(count_match.group(1))
    
    metadata = {
        "epoch": epoch,
        "root": root_bytes,
        "root_hex": bytes(root_bytes).hex(),
        "leaves_count": leaves_count
    }
    
    print(f"Epoch: {metadata['epoch']}")
    print(f"Root (hex): {metadata['root_hex']}")
    print(f"Leaves Count: {metadata['leaves_count']}")
    print()
    
    return True, metadata


def submit_to_solana(metadata: dict) -> bool:
    """提交 root 到 Solana"""
    print(f"[Step 3/3] Submitting root to Solana...")
    print()
    
    # 检查 Solana 依赖
    try:
        from solders.keypair import Keypair
        from solana.rpc.async_api import AsyncClient
        # from anchorpy import Program, Provider
    except ImportError:
        print("❌ Error: Required Python packages not found")
        print("Install: pip install solders solana anchorpy")
        return False
    
    # 加载管理员密钥
    with open(ADMIN_KEYPAIR_PATH, 'r') as f:
        keypair_data = json.load(f)
    
    admin_keypair = Keypair.from_bytes(bytes(keypair_data))
    
    print(f"Admin pubkey: {admin_keypair.pubkey()}")
    print()
    
    # 这里应该实现实际的 Solana 交易提交逻辑
    # 示例框架：
    
    """
    async def submit_transaction():
        client = AsyncClient(SOLANA_RPC_URL)
        
        # 加载 Program IDL
        # idl = await Program.load_idl(DISTRIBUTOR_PROGRAM_ID, client)
        # program = Program(idl, DISTRIBUTOR_PROGRAM_ID, Provider(client, admin_keypair))
        
        # 调用合约方法
        # tx = await program.rpc["update_epoch_root"](
        #     metadata['epoch'],
        #     metadata['root'],
        #     metadata['leaves_count'],
        #     ctx=Context(
        #         accounts={
        #             "distributor": DISTRIBUTOR_ADDRESS,
        #             "admin": admin_keypair.pubkey(),
        #         },
        #         signers=[admin_keypair]
        #     )
        # )
        
        # print(f"✅ Transaction signature: {tx}")
        # print(f"View on explorer: https://explorer.solana.com/tx/{tx}?cluster={SOLANA_NETWORK}")
        
        await client.close()
    
    # 运行异步函数
    import asyncio
    asyncio.run(submit_transaction())
    """
    
    print("⚠️  Note: Solana transaction submission not implemented yet")
    print("Please implement the actual Solana submission logic in this function")
    print()
    
    return True


def print_summary(metadata: dict):
    """打印摘要"""
    print("=" * 60)
    print("Epoch Submission Summary")
    print("=" * 60)
    print(f"Epoch: {metadata['epoch']}")
    print(f"Leaves Count: {metadata['leaves_count']}")
    print(f"Root (hex): {metadata['root_hex']}")
    print(f"Status: ⚠️  Pending implementation")
    print("=" * 60)
    print()
    print("Next steps:")
    print("1. Implement the Solana transaction submission logic")
    print("2. Deploy your Distributor contract to Solana")
    print("3. Update the configuration at the top of this script")
    print("4. Run this script again to submit the epoch")
    print()
    print("After successful submission:")
    print("- Verify on Solana Explorer")
    print("- Users can now claim their rewards via the frontend")


def main():
    """主函数"""
    if len(sys.argv) != 2:
        print("Usage: python3 epoch_submit.py <epoch_number>")
        print("Example: python3 epoch_submit.py 1")
        sys.exit(1)
    
    try:
        epoch = int(sys.argv[1])
    except ValueError:
        print("Error: epoch_number must be an integer")
        sys.exit(1)
    
    print("=" * 60)
    print("Epoch Snapshot Submit Script (Python)")
    print("=" * 60)
    print(f"Epoch: {epoch}")
    print(f"Backend Canister: {BACKEND_CANISTER_ID}")
    print(f"Solana Network: {SOLANA_NETWORK}")
    print("=" * 60)
    print()
    
    # Step 1: Build snapshot
    if not build_epoch_snapshot(epoch):
        sys.exit(1)
    
    # Step 2: Get metadata
    success, metadata = get_epoch_metadata(epoch)
    if not success:
        sys.exit(1)
    
    # Step 3: Submit to Solana
    if not submit_to_solana(metadata):
        sys.exit(1)
    
    # Print summary
    print_summary(metadata)


if __name__ == "__main__":
    main()
