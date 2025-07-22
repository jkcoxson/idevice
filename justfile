check-features:
  cd idevice
  cargo hack check --feature-powerset --no-dev-deps
  cd ..

ci-check: build-ffi-native build-tools-native build-cpp build-c
  cargo clippy --all-targets --all-features -- -D warnings
macos-ci-check: ci-check xcframework

[working-directory: 'ffi']
build-ffi-native:
  cargo build --release

[working-directory: 'tools']
build-tools-native:
  cargo build --release

create-example-build-folder:
  mkdir -p cpp/examples/build
  mkdir -p ffi/examples/build

[working-directory: 'cpp/examples/build']
build-cpp: build-ffi-native create-example-build-folder
  cmake .. && make

[working-directory: 'ffi/examples/build']
build-c: build-ffi-native create-example-build-folder
  cmake .. && make

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
    cargo build --release --target aarch64-apple-ios --features obfuscate

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

plist_xcframework: plist_clean build_plist_ios build_plist_sim build_plist_x86_64_sim build_plist_mac build_plist_x86_64_mac plist_merge_archs
    rm -rf {{lib_name}}.xcframework
    xcodebuild -create-xcframework \
        -framework {{ios_out}}/plist.framework \
        -framework build/universal-sim/plist.framework \
        -framework build/universal-mac/plist.framework \
        -output swift/{{lib_name}}.xcframework

plist_clean:
  rm -rf build
  rm -rf swift/plist.xcframework

plist_merge_archs:
    # Merge simulator dylibs (arm64 + x86_64)
    mkdir -p build/universal-sim
    lipo -create \
        {{sim_out}}/lib/libplist-2.0.4.dylib \
        {{x86_64_sim_out}}/lib/libplist-2.0.4.dylib \
        -output build/universal-sim/libplist-2.0.4.dylib

    mkdir -p build/universal-sim/plist.framework/Headers
    mkdir -p build/universal-sim/plist.framework/Modules
    cp build/universal-sim/libplist-2.0.4.dylib build/universal-sim/plist.framework/plist
    cp {{sim_out}}/include/plist/*.h build/universal-sim/plist.framework/Headers
    cp swift/Info.plist build/universal-sim/plist.framework/Info.plist
    cp swift/plistinclude/module.modulemap build/universal-sim/plist.framework/Modules/module.modulemap

    # Merge macOS dylibs (arm64 + x86_64)
    mkdir -p build/universal-mac
    lipo -create \
        {{mac_out}}/lib/libplist-2.0.4.dylib \
        {{x86_64_mac_out}}/lib/libplist-2.0.4.dylib \
        -output build/universal-mac/libplist-2.0.4.dylib

    mkdir -p build/universal-mac/plist.framework/Headers
    mkdir -p build/universal-mac/plist.framework/Modules
    cp build/universal-mac/libplist-2.0.4.dylib build/universal-mac/plist.framework/plist
    cp {{mac_out}}/include/plist/*.h build/universal-mac/plist.framework/Headers
    cp swift/Info.plist build/universal-mac/plist.framework/Info.plist
    cp swift/plistinclude/module.modulemap build/universal-mac/plist.framework/Modules/module.modulemap

build_plist_ios:
    rm -rf {{ios_out}} build/build-ios
    rm -rf build/ios
    mkdir -p {{ios_out}}
    mkdir -p build/build-ios && cd build/build-ios && \
      ../../ffi/libplist/autogen.sh \
      --host=arm-apple-darwin \
      --prefix="$(pwd)/../../{{ios_out}}" \
      --without-cython \
      --without-tools \
    CC="$(xcrun --sdk iphoneos --find clang)" \
    CFLAGS="-arch arm64 -isysroot $(xcrun --sdk iphoneos --show-sdk-path) -mios-version-min=12.0" \
    CXX="$(xcrun --sdk iphoneos --find clang++)" \
    CXXFLAGS="-arch arm64 -isysroot $(xcrun --sdk iphoneos --show-sdk-path) -mios-version-min=12.0" \
    LDFLAGS="-arch arm64 -isysroot $(xcrun --sdk iphoneos --show-sdk-path) -mios-version-min=12.0" && \
      make clean && make -j$(sysctl -n hw.ncpu) && make install

    install_name_tool -id @rpath/plist.framework/plist {{ios_out}}/lib/libplist-2.0.4.dylib

    mkdir -p {{ios_out}}/plist.framework/Headers
    mkdir -p {{ios_out}}/plist.framework/Modules
    cp {{ios_out}}/lib/libplist-2.0.4.dylib {{ios_out}}/plist.framework/plist
    cp {{ios_out}}/include/plist/*.h {{ios_out}}/plist.framework/Headers
    cp swift/Info.plist {{ios_out}}/plist.framework/Info.plist
    cp swift/plistinclude/module.modulemap {{ios_out}}/plist.framework/Modules/module.modulemap

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
    CFLAGS="-arch arm64 -isysroot $(xcrun --sdk iphonesimulator --show-sdk-path) -mios-simulator-version-min=12.0" \
    CXX="$(xcrun --sdk iphonesimulator --find clang++)" \
    CXXFLAGS="-arch arm64 -isysroot $(xcrun --sdk iphonesimulator --show-sdk-path) -mios-simulator-version-min=12.0" \
    LDFLAGS="-arch arm64 -isysroot $(xcrun --sdk iphonesimulator --show-sdk-path) -mios-simulator-version-min=12.0" && \
      make clean && make -j$(sysctl -n hw.ncpu) && make install

    install_name_tool -id @rpath/plist.framework/plist {{sim_out}}/lib/libplist-2.0.4.dylib

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
    CFLAGS="-arch x86_64 -isysroot $(xcrun --sdk iphonesimulator --show-sdk-path) -mios-simulator-version-min=12.0" \
    CXX="$(xcrun --sdk iphonesimulator --find clang++)" \
    CXXFLAGS="-arch x86_64 -isysroot $(xcrun --sdk iphonesimulator --show-sdk-path) -mios-simulator-version-min=12.0" \
    LDFLAGS="-arch x86_64 -isysroot $(xcrun --sdk iphonesimulator --show-sdk-path) -mios-simulator-version-min=12.0" && \
      make clean && make -j$(sysctl -n hw.ncpu) && make install

    install_name_tool -id @rpath/plist.framework/plist {{x86_64_sim_out}}/lib/libplist-2.0.4.dylib

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
    CFLAGS="-arch arm64 -isysroot $(xcrun --sdk macosx --show-sdk-path) -mmacosx-version-min=11.0" \
    CXX="$(xcrun --sdk macosx --find clang++)" \
    CXXFLAGS="-arch arm64 -isysroot $(xcrun --sdk macosx --show-sdk-path) -mmacosx-version-min=11.0" \
    LDFLAGS="-arch arm64 -isysroot $(xcrun --sdk macosx --show-sdk-path) -mmacosx-version-min=11.0" && \
      make clean && make -j$(sysctl -n hw.ncpu) && make install

    install_name_tool -id @rpath/plist.framework/plist {{mac_out}}/lib/libplist-2.0.4.dylib

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
    CFLAGS="-arch x86_64 -isysroot $(xcrun --sdk macosx --show-sdk-path) -mmacosx-version-min=11.0" \
    CXX="$(xcrun --sdk macosx --find clang++)" \
    CXXFLAGS="-arch x86_64 -isysroot $(xcrun --sdk macosx --show-sdk-path) -mmacosx-version-min=11.0" \
    LDFLAGS="-arch x86_64 -isysroot $(xcrun --sdk macosx --show-sdk-path) -mmacosx-version-min=11.0" && \
      make clean && make -j$(sysctl -n hw.ncpu) && make install

    install_name_tool -id @rpath/plist.framework/plist {{x86_64_mac_out}}/lib/libplist-2.0.4.dylib
