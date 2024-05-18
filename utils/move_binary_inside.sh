#!/usr/bin/env bash 
## to be executed once you get zip files containing binairies from github CI and put them in /tmp/gupaxx_*

[[ -d skel ]]; check "skel"
[[ -f linux.zip ]]; check "linux zip"
[[ -f windows.zip ]]; check "windows zip"
[[ -f macos.zip ]]; check "macos zip"
unzip linux.zip; unzip macos.zip; unzip windows.zip
tar -xf windows.tar
mv gupaxx.exe skel/windows/Gupaxx.exe
mv gupaxx_b.exe skel/windows_b/Gupaxx.exe
tar -xf linux.tar
mv gupaxx skel/linux/gupaxx
mv gupaxx_b skel/linux_b/gupaxx
tar -xf macos.tar
mv Gupaxx-macos-x64.app/Contents/Info.plist skel/macos-x64/Gupaxx.app/Contents/Info.plist
mv Gupaxx-macos-x64.app/Contents/MacOS/gupaxx skel/macos-x64/Gupaxx.app/Contents/MacOS/gupaxx
mv Gupaxx-macos-x64.app_b/Contents/Info.plist skel/macos-x64_b/Gupaxx.app/Contents/Info.plist
mv Gupaxx-macos-x64.app_b/Contents/MacOS/gupaxx skel/macos-x64_b/Gupaxx.app/Contents/MacOS/gupaxx
mv Gupaxx-macos-arm64.app/Contents/Info.plist skel/macos-arm64/Gupaxx.app/Contents/Info.plist
mv Gupaxx-macos-arm64.app/Contents/MacOS/gupaxx skel/macos-arm64/Gupaxx.app/Contents/MacOS/gupaxx
mv Gupaxx-macos-arm64.app_b/Contents/Info.plist skel/macos-arm64_b/Gupaxx.app/Contents/Info.plist
mv Gupaxx-macos-arm64.app_b/Contents/MacOS/gupaxx skel/macos-arm64_b/Gupaxx.app/Contents/MacOS/gupaxx
rm -r Gupaxx-macos-x64.app
rm -r Gupaxx-macos-arm64.app
rm -r Gupaxx-macos-x64.app_b
rm -r Gupaxx-macos-arm64.app_b
rm linux.zip; rm macos.zip; rm windows.zip
# windows unzip only the exe so not tar to delete.
rm linux.tar; rm macos.tar; rm windows.tar
