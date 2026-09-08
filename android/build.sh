#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
WORKSPACE=$(CDPATH= cd -- "$ROOT/../.." && pwd)
SDK="$WORKSPACE/android-sdk"
JDK="$WORKSPACE/toolchains/jdk-17.0.20+8"
BUILD_TOOLS="$SDK/build-tools/34.0.0"
PLATFORM="$SDK/platforms/android-34/android.jar"

APP="$ROOT/app/src/main"
OUT="$ROOT/build"
GEN="$OUT/generated"
CLASSES="$OUT/classes"
DEX="$OUT/dex"
UNALIGNED="$OUT/ottotree-audit-unaligned.apk"
ALIGNED="$OUT/ottotree-audit-aligned.apk"
APK="$OUT/ottotree-audit-debug.apk"
KEYSTORE="$OUT/debug.keystore"

rm -rf "$OUT"
mkdir -p "$GEN" "$CLASSES" "$DEX"

"$BUILD_TOOLS/aapt2" compile --dir "$APP/res" -o "$OUT/resources.zip"
"$BUILD_TOOLS/aapt2" link \
    -I "$PLATFORM" \
    --manifest "$APP/AndroidManifest.xml" \
    --java "$GEN" \
    -o "$UNALIGNED" \
    "$OUT/resources.zip"

find "$APP/java" "$GEN" -name '*.java' > "$OUT/sources.txt"
"$JDK/bin/javac" -source 8 -target 8 -bootclasspath "$PLATFORM" -d "$CLASSES" @"$OUT/sources.txt"
(cd "$CLASSES" && "$JDK/bin/jar" cf "$OUT/classes.jar" .)
"$BUILD_TOOLS/d8" --lib "$PLATFORM" --output "$DEX" "$OUT/classes.jar"
"$BUILD_TOOLS/aapt2" link \
    -I "$PLATFORM" \
    --manifest "$APP/AndroidManifest.xml" \
    -o "$UNALIGNED" \
    "$OUT/resources.zip"
(cd "$DEX" && zip -q "$UNALIGNED" classes.dex)
"$BUILD_TOOLS/zipalign" -f 4 "$UNALIGNED" "$ALIGNED"

"$JDK/bin/keytool" -genkeypair \
    -keystore "$KEYSTORE" \
    -storepass android \
    -keypass android \
    -alias androiddebugkey \
    -keyalg RSA \
    -keysize 2048 \
    -validity 10000 \
    -dname "CN=Android Debug,O=Android,C=US" >/dev/null 2>&1

"$BUILD_TOOLS/apksigner" sign \
    --ks "$KEYSTORE" \
    --ks-pass pass:android \
    --key-pass pass:android \
    --out "$APK" \
    "$ALIGNED"

"$BUILD_TOOLS/apksigner" verify "$APK"
echo "$APK"
