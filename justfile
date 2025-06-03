check-features:
  cd idevice
  cargo hack check --feature-powerset --no-dev-deps
  cd ..

xcframework: apple-build
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

lib_name := "plist"
src_dir := "ffi/libplist"

ios_out := "build/ios"
sim_out := "build/sim"
x86_64_sim_out := "build/x86_64_sim"
mac_out := "build/mac"
x86_64_mac_out := "build/x86_64_mac"

plist_xcframework: build_plist_ios build_plist_sim build_plist_x86_64_sim build_plist_mac build_plist_x86_64_mac merge_archs
    rm -rf {{lib_name}}.xcframework
    xcodebuild -create-xcframework \
        -library {{ios_out}}/lib/libplist-2.0.dylib -headers {{ios_out}}/include \
        -library build/universal-sim/libplist-2.0.dylib -headers {{sim_out}}/include \
        -library build/universal-mac/libplist-2.0.dylib -headers {{mac_out}}/include \
        -output swift/{{lib_name}}.xcframework

merge_archs:
    # Merge simulator dylibs (arm64 + x86_64)
    mkdir -p build/universal-sim
    lipo -create \
        {{sim_out}}/lib/libplist-2.0.dylib \
        {{x86_64_sim_out}}/lib/libplist-2.0.dylib \
        -output build/universal-sim/libplist-2.0.dylib

    # Merge macOS dylibs (arm64 + x86_64)
    mkdir -p build/universal-mac
    lipo -create \
        {{mac_out}}/lib/libplist-2.0.dylib \
        {{x86_64_mac_out}}/lib/libplist-2.0.dylib \
        -output build/universal-mac/libplist-2.0.dylib

build_plist_ios:
    rm -rf {{ios_out}} build/build-ios
    mkdir -p {{ios_out}}
    mkdir -p build/build-ios && cd build/build-ios && \
      ../../ffi/libplist/autogen.sh \
      --host=arm-apple-darwin \
      --prefix="$(pwd)/../../{{ios_out}}" \
      --without-cython \
      --without-tools \
    CC="$(xcrun --sdk iphoneos --find clang)" \
    CFLAGS="-arch arm64 -isysroot $(xcrun --sdk iphoneos --show-sdk-path)" \
    CXX="$(xcrun --sdk iphoneos --find clang++)" \
    CXXFLAGS="-arch arm64 -isysroot $(xcrun --sdk iphoneos --show-sdk-path)" \
    LDFLAGS="-arch arm64 -isysroot $(xcrun --sdk iphoneos --show-sdk-path)" && \
      make clean && make -j$(sysctl -n hw.ncpu) && make install

build_plist_sim:
    rm -rf {{sim_out}} build/build-sim
    mkdir -p {{sim_out}}
    mkdir -p build/build-sim && cd build/build-sim && \
      ../../ffi/libplist/autogen.sh \
      --host=arm-apple-darwin \
      --prefix="$(pwd)/../../{{sim_out}}" \
      --without-cython \
      --without-tools \
    CC="$(xcrun --sdk iphonesimulator --find clang)" \
    CFLAGS="-arch arm64 -isysroot $(xcrun --sdk iphonesimulator --show-sdk-path)" \
    CXX="$(xcrun --sdk iphonesimulator --find clang++)" \
    CXXFLAGS="-arch arm64 -isysroot $(xcrun --sdk iphonesimulator --show-sdk-path)" \
    LDFLAGS="-arch arm64 -isysroot $(xcrun --sdk iphonesimulator --show-sdk-path)" && \
      make clean && make -j$(sysctl -n hw.ncpu) && make install

build_plist_x86_64_sim:
    rm -rf {{x86_64_sim_out}} build/build-sim
    mkdir -p {{x86_64_sim_out}}
    mkdir -p build/build-sim && cd build/build-sim && \
      ../../ffi/libplist/autogen.sh \
      --host=x86_64-apple-darwin \
      --prefix="$(pwd)/../../{{x86_64_sim_out}}" \
      --without-cython \
      --without-tools \
    CC="$(xcrun --sdk iphonesimulator --find clang)" \
    CFLAGS="-arch x86_64 -isysroot $(xcrun --sdk iphonesimulator --show-sdk-path)" \
    CXX="$(xcrun --sdk iphonesimulator --find clang++)" \
    CXXFLAGS="-arch x86_64 -isysroot $(xcrun --sdk iphonesimulator --show-sdk-path)" \
    LDFLAGS="-arch x86_64 -isysroot $(xcrun --sdk iphonesimulator --show-sdk-path)" && \
      make clean && make -j$(sysctl -n hw.ncpu) && make install

build_plist_mac:
    rm -rf {{mac_out}} build/build-mac
    mkdir -p {{mac_out}}
    mkdir -p build/build-mac && cd build/build-mac && \
      ../../ffi/libplist/autogen.sh \
      --host=aarch64-apple-darwin \
      --prefix="$(pwd)/../../{{mac_out}}" \
      --without-cython \
      --without-tools \
    CC="$(xcrun --sdk macosx --find clang)" \
    CFLAGS="-arch arm64 -isysroot $(xcrun --sdk macosx --show-sdk-path)" \
    CXX="$(xcrun --sdk macosx --find clang++)" \
    CXXFLAGS="-arch arm64 -isysroot $(xcrun --sdk macosx --show-sdk-path)" \
    LDFLAGS="-arch arm64 -isysroot $(xcrun --sdk macosx --show-sdk-path)" && \
      make clean && make -j$(sysctl -n hw.ncpu) && make install

build_plist_x86_64_mac:
    rm -rf {{x86_64_mac_out}} build/build-mac
    mkdir -p {{x86_64_mac_out}}
    mkdir -p build/build-mac && cd build/build-mac && \
      ../../ffi/libplist/autogen.sh \
      --host=x86_64-apple-darwin \
      --prefix="$(pwd)/../../{{x86_64_mac_out}}" \
      --without-cython \
      --without-tools \
    CC="$(xcrun --sdk macosx --find clang)" \
    CFLAGS="-arch x86_64 -isysroot $(xcrun --sdk macosx --show-sdk-path)" \
    CXX="$(xcrun --sdk macosx --find clang++)" \
    CXXFLAGS="-arch x86_64 -isysroot $(xcrun --sdk macosx --show-sdk-path)" \
    LDFLAGS="-arch x86_64 -isysroot $(xcrun --sdk macosx --show-sdk-path)" && \
      make clean && make -j$(sysctl -n hw.ncpu) && make install
