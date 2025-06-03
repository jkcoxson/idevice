check-features:
  cd idevice
  cargo hack check --feature-powerset --no-dev-deps
  cd ..

xcframework: # apple-build
  rm -rf swift/IDevice.xcframework
  rm -rf swift/libs
  cp ffi/idevice.h swift/include/idevice.h
  mkdir swift/libs
  lipo -create -output swift/libs/idevice-ios-sim.a \
    target/aarch64-apple-ios-sim/release/libidevice_ffi.a \
    target/x86_64-apple-ios/release/libidevice_ffi.a
  lipo -create -output swift/libs/idevice-macos.a \
    target/aarch64-apple-darwin/release/libidevice_ffi.a \
    target/x86_64-apple-darwin/release/libidevice_ffi.a

  xcodebuild -create-xcframework \
    -library target/aarch64-apple-ios/release/libidevice_ffi.a -headers swift/include \
    -library swift/libs/idevice-ios-sim.a -headers swift/include \
    -library swift/libs/idevice-macos.a -headers swift/include \
    -output swift/IDevice.xcframework
  
  zip -r bundle.zip IDevice.xcframework
  openssl dgst -sha256 bundle.zip

[working-directory: 'ffi']
apple-build: # requires a Mac
  # iOS device build
  BINDGEN_EXTRA_CLANG_ARGS="--sysroot=$(xcrun --sdk iphoneos --show-sdk-path)" \
    cargo build --release --target aarch64-apple-ios

  # iOS Simulator (arm64)
  BINDGEN_EXTRA_CLANG_ARGS="--sysroot=$(xcrun --sdk iphonesimulator --show-sdk-path)" \
    cargo build --release --target aarch64-apple-ios-sim

  # iOS Simulator (x86_64)
  BINDGEN_EXTRA_CLANG_ARGS="--sysroot=$(xcrun --sdk iphonesimulator --show-sdk-path)" \
    cargo build --release --target x86_64-apple-ios

  # macOS (native) – no special env needed
  cargo build --release --target aarch64-apple-darwin
  cargo build --release --target x86_64-apple-darwin
