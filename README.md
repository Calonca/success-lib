cargo build --release
cargo run --bin uniffi-bindgen generate --library target/release/libsuccesslib.so --language kotlin --out-dir out


# Setup rust as described here
https://sal.dev/android/intro-rust-android-uniffi/

Open (or create) your $HOME/.cargo/config file. Add each of the target linkers. Please note:

The path has to be absolute.
armv7a’s target name and clang name are different and it is “androideabi” as opposed to “android”.
# ~/.cargo/config
# ...
[target.x86_64-linux-android]
linker = "/Users/sal/Library/Android/sdk/ndk/25.2.9519653/toolchains/llvm/prebuilt/darwin-x86_64/bin/x86_64-linux-android24-clang"

[target.i686-linux-android]
linker = "/Users/sal/Library/Android/sdk/ndk/25.2.9519653/toolchains/llvm/prebuilt/darwin-x86_64/bin/i686-linux-android24-clang"

[target.armv7-linux-androideabi]
linker = "/Users/sal/Library/Android/sdk/ndk/25.2.9519653/toolchains/llvm/prebuilt/darwin-x86_64/bin/armv7a-linux-androideabi24-clang"

[target.aarch64-linux-android]
linker = "/Users/sal/Library/Android/sdk/ndk/25.2.9519653/toolchains/llvm/prebuilt/darwin-x86_64/bin/aarch64-linux-android24-clang"
Finally, add the targets to your Rust environment.

❯ rustup target add \
    x86_64-linux-android \
    i686-linux-android \
    armv7-linux-androideabi \
    aarch64-linux-android

### Make lib
cargo build --lib \
    --target x86_64-linux-android \
    --target i686-linux-android \
    --target armv7-linux-androideabi \
    --target aarch64-linux-android