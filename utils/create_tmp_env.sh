#!/usr/bin/env bash

# Sets up a packaging environment in [/tmp]

# Make sure we're in the [gupaxx/utils] directory
set -ex
[[ $PWD = */gupaxx ]]

# Make sure the folder doesn't already exist
GIT_COMMIT=$(cat .git/refs/heads/main)
FOLDER="gupaxx_${GIT_COMMIT}"
[[ ! -e /tmp/${FOLDER} ]]

mkdir /tmp/${FOLDER}
cp -r utils/* /tmp/${FOLDER}/
cp CHANGELOG.md /tmp/${FOLDER}/skel/

## Download XMRig Binaries
# download xmrig into directory linux
wget https://github.com/xmrig/xmrig/releases/download/v6.21.1/xmrig-6.21.1-linux-static-x64.tar.gz
tar -xf xmrig-6.21.1-linux-static-x64.tar.gz
mv xmrig-6.21.1/xmrig /tmp/${FOLDER}/skel/linux_b/xmrig/xmrig
rm -r xmrig-6.21.1
rm xmrig-6.21.1-linux-static-x64.tar.gz 
# download xmrig into directory macos-arm64
wget https://github.com/xmrig/xmrig/releases/download/v6.21.1/xmrig-6.21.1-macos-arm64.tar.gz
tar -xf xmrig-6.21.1-macos-arm64.tar.gz
mv xmrig-6.21.1/xmrig /tmp/${FOLDER}/skel/macos-arm64_b/Gupaxx.app/Contents/MacOS/xmrig/xmrig
rm -r xmrig-6.21.1
rm xmrig-6.21.1-macos-arm64.tar.gz
# download xmrig into directory macos-x64
wget https://github.com/xmrig/xmrig/releases/download/v6.21.1/xmrig-6.21.1-macos-x64.tar.gz
tar -xf xmrig-6.21.1-macos-x64.tar.gz
mv xmrig-6.21.1/xmrig /tmp/${FOLDER}/skel/macos-x64_b/Gupaxx.app/Contents/MacOS/xmrig/xmrig
rm -r xmrig-6.21.1
rm xmrig-6.21.1-macos-x64.tar.gz
# download xmrig into directory windows
wget https://github.com/xmrig/xmrig/releases/download/v6.21.1/xmrig-6.21.1-msvc-win64.zip
unzip xmrig-6.21.1-msvc-win64.zip
mv xmrig-6.21.1/xmrig.exe /tmp/${FOLDER}/skel/windows_b/XMRig/xmrig.exe
rm -r xmrig-6.21.1
rm xmrig-6.21.1-msvc-win64.zip

## Download P2Pool Binaries
# download p2pool into directory linux
wget https://github.com/SChernykh/p2pool/releases/download/v3.10/p2pool-v3.10-linux-x64.tar.gz
tar -xf p2pool-v3.10-linux-x64.tar.gz
mv p2pool-v3.10-linux-x64/p2pool /tmp/${FOLDER}/skel/linux_b/p2pool/p2pool
rm -r p2pool-v3.10-linux-x64
rm p2pool-v3.10-linux-x64.tar.gz
# download p2pool into directory macos-arm64
wget https://github.com/SChernykh/p2pool/releases/download/v3.10/p2pool-v3.10-macos-aarch64.tar.gz
tar -xf p2pool-v3.10-macos-aarch64.tar.gz
mv p2pool-v3.10-macos-aarch64/p2pool /tmp/${FOLDER}/skel/macos-arm64_b/Gupaxx.app/Contents/MacOS/p2pool/p2pool
rm -r p2pool-v3.10-macos-aarch64
rm p2pool-v3.10-macos-aarch64.tar.gz
# download p2pool into directory macos-x64
wget https://github.com/SChernykh/p2pool/releases/download/v3.10/p2pool-v3.10-macos-x64.tar.gz
tar -xf p2pool-v3.10-macos-x64.tar.gz
mv p2pool-v3.10-macos-x64/p2pool /tmp/${FOLDER}/skel/macos-x64_b/Gupaxx.app/Contents/MacOS/p2pool/p2pool
rm -r p2pool-v3.10-macos-x64
rm p2pool-v3.10-macos-x64.tar.gz
# download p2pool into directory windows
wget https://github.com/SChernykh/p2pool/releases/download/v3.10/p2pool-v3.10-windows-x64.zip
unzip p2pool-v3.10-windows-x64.zip
mv p2pool-v3.10-windows-x64/p2pool.exe /tmp/${FOLDER}/skel/windows_b/P2Pool/p2pool.exe
rm -r p2pool-v3.10-windows-x64
rm p2pool-v3.10-windows-x64.zip

set +ex

echo
ls --color=always /tmp/${FOLDER}
echo "/tmp/${FOLDER} ... OK"
