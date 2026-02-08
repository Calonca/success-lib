# Success Library (success-lib)

The core engine for building effort- and rewards-driven applications.

## The Problem
Learning a new language, mastering an instrument, or launching a side project requires immense effort and time. In a world where apps are engineered to capture our attention, accomplishing deep work is harder than ever.

Habit trackers, app blockers, and pomodoro timers often fail because they are easy to bypass or provide rewards that don't feel meaningful.

**Success Library** gives you the tools to solve this. It's a cross-platform library that tracks focus sessions and allows your apps to unlock rewards that actually matter to the user.

---

# Why use this library?
> *Focus on the user experience, not the boilerplate.*

Instead of reimplementing data structures for goals, sessions, and persistence, you can use the library to build personalized experiences.

### Key Features
- **Universal Compatibility**: Built in Rust with **UniFFI** and **WASM** support. Targeted at Android (Kotlin), iOS (Swift), Web (TypeScript), and Desktop.
- **Human-Readable DB**: Uses `Markdown` and `YAML`. Users own their data and can edit it with any text editor—no vendor lock-in.
- **Interoperability**: Share a single goal archive across devices and between different applications. Multiple apps on your phone or PC can work on the same goals and sessions. For example, a study session recorded in one app can unlock a reward in another app on the same device, or even across different platforms like your PC.
---

# How to Use It
The library is designed to be the backbone of your application. Whether you are building with **Tauri**, **React Native**, or **Compose Multiplatform**, integration is possible.
For example making a desktop app with Tauri can be done by adding the rust library and calling the functions inside it. For more examples look at other apps in the ecosystem section.

### Vibe Coding Prompt
If you're using an AI assistant to build your app, try this prompt to build a desktop app:
```text
Build a desktop application using Tauri that integrates https://github.com/Calonca/success-lib to manage goals and focus sessions. 
Look at https://github.com/Calonca/success-cli for a reference implementation.
The app should be personalized for {create a scenario, e.g. a student that wants to study for 4 hours a day}
```

---

# Ecosystem
- A CLI app for power users that associates goals to workspaces and opens apps based on the goal. Also used as a reference implementation of the library. https://github.com/Calonca/success-cli
- Web demo of the previous CLI app. (link will be posted soon)

- An android launcher focused on requiring only one click to start a session and unlocking apps marked as rewards when a session is completed.

- A react native app for android and ios where by tracking your goals you can earn coins to buy accessories for your avatar.
---

# Developer Guide: Building Manually
To build the core library:
```bash
cargo build --release
```

### Building for Android
Generating Kotlin bindings requires `uniffi-bindgen`.

To build for android:
```
cargo build --release
cargo run --bin uniffi-bindgen generate --library target/release/libsuccesslib.so --language kotlin --out-dir out
```

#### Setup rust as described here
https://sal.dev/android/intro-rust-android-uniffi/

Open (or create) your $HOME/.cargo/config file. Add each of the target linkers. Please note:

The path has to be absolute.
armv7a’s target name and clang name are different and it is “androideabi” as opposed to “android”.

Modify ~/.cargo/config
```toml
[target.x86_64-linux-android]
linker = "/Users/sal/Library/Android/sdk/ndk/25.2.9519653/toolchains/llvm/prebuilt/darwin-x86_64/bin/x86_64-linux-android24-clang"

[target.i686-linux-android]
linker = "/Users/sal/Library/Android/sdk/ndk/25.2.9519653/toolchains/llvm/prebuilt/darwin-x86_64/bin/i686-linux-android24-clang"

[target.armv7-linux-androideabi]
linker = "/Users/sal/Library/Android/sdk/ndk/25.2.9519653/toolchains/llvm/prebuilt/darwin-x86_64/bin/armv7a-linux-androideabi24-clang"

[target.aarch64-linux-android]
linker = "/Users/sal/Library/Android/sdk/ndk/25.2.9519653/toolchains/llvm/prebuilt/darwin-x86_64/bin/aarch64-linux-android24-clang"
Finally, add the targets to your Rust environment.
```
Then run the commands
```bash
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
```