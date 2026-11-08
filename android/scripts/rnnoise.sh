#!/bin/bash
# Compila a ponte JNI do RNNoise (android/native/rnnoise, Rust + nnnoiseless, o mesmo supressor do desktop).
#
#   scripts/rnnoise.sh          # arm64-v8a (vai para o APK) + a versão do PC (usada só pelos testes na JVM)
#   scripts/rnnoise.sh --host   # só a do PC (rápido, não precisa de NDK)
#
# Saídas:
#   app/src/main/jniLibs/arm64-v8a/libtelacast_rnnoise.so   VERSIONADA no git: o Gradle só empacota, não precisa de NDK nem de Rust
#   native/rnnoise/target/release/libtelacast_rnnoise.so    do PC, NÃO versionada; os testes de unidade a usam se existir
#
# Pré-requisitos só para a arm64 (uma vez; ver docs no PROGRESS.md):
#   sdkmanager --install "ndk;27.2.12479018"    e    rustup target add aarch64-linux-android
# Sem a .so no APK o app cai sozinho para o NoiseSuppressor do sistema (se o aparelho tiver).
set -euo pipefail
cd "$(dirname "$0")/../native/rnnoise"
NDK_VERSAO="${NDK_VERSAO:-27.2.12479018}"
SDK="${ANDROID_HOME:-$HOME/Android/Sdk}"
API=26   # = minSdk do app

nice -n 19 cargo build --release -j2
echo "ok (PC): $(pwd)/target/release/libtelacast_rnnoise.so"
[ "${1:-}" = "--host" ] && exit 0

NDK="$SDK/ndk/$NDK_VERSAO"
[ -d "$NDK" ] || { echo "falta o NDK $NDK_VERSAO: $SDK/cmdline-tools/latest/bin/sdkmanager --install \"ndk;$NDK_VERSAO\"" >&2; exit 2; }
CLANG="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android${API}-clang"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CLANG"
export CC_aarch64_linux_android="$CLANG"
export AR_aarch64_linux_android="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
# páginas de 16 KB: exigência de aparelhos novos (Android 15+) para apps que miram o 15; inofensivo nos de 4 KB
export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"
nice -n 19 cargo build --release -j2 --target aarch64-linux-android
DESTINO="../../app/src/main/jniLibs/arm64-v8a"
mkdir -p "$DESTINO"
cp target/aarch64-linux-android/release/libtelacast_rnnoise.so "$DESTINO/"
ls -la "$DESTINO/libtelacast_rnnoise.so"
