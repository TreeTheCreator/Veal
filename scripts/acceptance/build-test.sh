# bash build-test.sh <feed-dir> <version>...
# Builds signed installers under the test identity (tauri.test.conf.json:
# "Veal Test", com.veal.test, local feed) and copies them and their .sig into
# <feed-dir>. Needs the updater key at ~/.tauri/veal-updater.key. About 9
# minutes each.
set -e
feed="$1"; shift
cd "$(dirname "$0")/../.."
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/veal-updater.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
mkdir -p "$feed"
for v in "$@"; do
  pnpm bundle --config src-tauri/tauri.test.conf.json --config "{\"version\":\"$v\"}"
  out="src-tauri/target/release/bundle/nsis/Veal Test_${v}_x64-setup.exe"
  cp "$out" "$out.sig" "$feed/"
done
echo BUILT "$@"
