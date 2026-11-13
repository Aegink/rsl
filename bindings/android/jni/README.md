# Android JNI 薄壳（T6.3）

内核以 `.so`（`cargo ndk -t arm64-v8a -t armeabi-v7a -o jniLibs build --release`）提供，
`rsl_jni.rs` 暴露 `RslNative` 四个 native 方法；Kotlin 层 `RslSource.kt` 生命周期管理。

```
Kotlin RslSource（生命周期/JSON↔Map）
        ↓ JNI（rsl_jni.rs）
librsl_core.so（rsl_core + ureq 传输）
```

构建步骤：
1. `rustup target add aarch64-linux-android`
2. `cargo install cargo-ndk`
3. `cargo ndk -t arm64-v8a -o app/src/main/jniLibs build --release -p rsl-ffi`
4. 拷贝 `RslSource.kt` 到 `app/src/main/java/…`
