#!/bin/bash
set -e

echo "=== Installing Aiken on WSL ==="
curl -sSfL "https://github.com/aiken-lang/aiken/releases/download/v1.1.23/aiken-x86_64-unknown-linux-musl.tar.gz" -o /tmp/aiken.tar.gz
tar -xzf /tmp/aiken.tar.gz -C /tmp
export PATH="/tmp/aiken-x86_64-unknown-linux-musl:$PATH"
aiken --version

echo "=== Creating fresh project ==="
mkdir -p /tmp/escrow-build
cd /tmp/escrow-build
aiken new thirdman/escrow
cd escrow

echo "=== Writing validator ==="
cp /mnt/d/third-man-app/onchain/escrow-real.ak validators/placeholder.ak

echo "=== Building real validator ==="
aiken build

echo "=== Extracting script hash ==="
python3 -c "import json; bp=json.load(open('plutus.json')); [print('REAL_SCRIPT_HASH=' + v['hash']) for v in bp['validators'] if 'spend' in v['title']]" 2>/dev/null || grep -o '"hash":"[^"]*"' plutus.json | head -1

echo "=== Copying plutus.json to D: ==="
cp plutus.json /mnt/d/third-man-app/onchain/plutus.json
cp validators/placeholder.ak /mnt/d/third-man-app/onchain/escrow.ak

echo "=== DONE ==="