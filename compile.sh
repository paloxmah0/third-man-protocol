#!/bin/bash
set -e
cd /tmp
export PATH="/tmp/aiken-x86_64-unknown-linux-musl:$PATH"
aiken --version

rm -rf /tmp/escrow-build
mkdir -p /tmp/escrow-build
cd /tmp/escrow-build
aiken new thirdman/escrow
cd escrow

cp /mnt/d/third-man-app/onchain/escrow-real.ak validators/placeholder.ak

echo "=== Building ==="
aiken build

echo "=== Result ==="
ls -la plutus.json

python3 << 'PYEOF'
import json
with open('plutus.json') as f:
    bp = json.load(f)
for v in bp['validators']:
    if 'spend' in v['title']:
        print('REAL_SCRIPT_HASH=' + v['hash'])
        print('COMPILED_CODE=' + v['compiledCode'][:80])
        break
PYEOF

cp plutus.json /mnt/d/third-man-app/onchain/plutus.json
cp validators/placeholder.ak /mnt/d/third-man-app/onchain/escrow.ak
echo "=== DONE ==="