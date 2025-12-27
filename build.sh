cargo build --release
cargo run --bin uniffi-bindgen generate --library target/release/libsuccesslib.so --language kotlin --out-dir out --config uniffi.toml


cargo build --lib --release \
    --target x86_64-linux-android \
    --target i686-linux-android \
    --target armv7-linux-androideabi \
    --target aarch64-linux-android


export libsdir="../lawnchair/lawnchair/src/app/lawnchair/jniLibs"
mkdir -p ${libsdir}/arm64-v8a
mkdir -p ${libsdir}/armeabi-v7a
mkdir -p ${libsdir}/x86_64
mkdir -p ${libsdir}/x86
cp target/aarch64-linux-android/release/libsuccesslib.so ${libsdir}/arm64-v8a/
cp target/armv7-linux-androideabi/release/libsuccesslib.so ${libsdir}/armeabi-v7a/
cp target/x86_64-linux-android/release/libsuccesslib.so ${libsdir}/x86_64/
cp target/i686-linux-android/release/libsuccesslib.so ${libsdir}/x86/

cp out/uniffi/successlib/successlib.kt ../lawnchair/lawnchair/src/app/lawnchair/successlib.kt