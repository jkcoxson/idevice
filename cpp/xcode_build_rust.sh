#!/bin/sh

cp ../ffi/idevice.h include/

# This script builds a Rust library for use in an Xcode project.
# It's designed to be used as a "Run Script" build phase on a native Xcode target.
# It handles multiple architectures by building for each and combining them with `lipo`.

# --- Configuration ---
# The name of your Rust crate (the name in Cargo.toml)
CRATE_NAME="idevice_ffi"
# The path to your Rust project's root directory (containing Cargo.toml)
RUST_PROJECT_PATH="${PROJECT_DIR}/../ffi"

# --- Environment Setup ---
# Augment the PATH to include common locations for build tools like cmake and go.
export PATH="/opt/homebrew/bin:/usr/local/bin:/usr/local/go/bin:$PATH"

# --- Locate Cargo ---
# Xcode's build environment often has a minimal PATH, so we need to find cargo explicitly.
if [ -x "${HOME}/.cargo/bin/cargo" ]; then
  CARGO="${HOME}/.cargo/bin/cargo"
else
  CARGO=$(command -v cargo)
  if [ -z "$CARGO" ]; then
    echo "Error: cargo executable not found." >&2
    echo "Please ensure Rust is installed and cargo is in your PATH or at ~/.cargo/bin/" >&2
    exit 1
  fi
fi

# --- Script Logic ---

# Exit immediately if a command exits with a non-zero status.
set -e

# --- Platform & SDK Configuration ---
# In a "Run Script" phase on a native target, PLATFORM_NAME is reliable.
# We use it to determine the correct SDK and build parameters.
PLATFORM_SUFFIX=""
SDK_NAME=""

if [ "$PLATFORM_NAME" = "iphoneos" ]; then
  PLATFORM_SUFFIX="-iphoneos"
  SDK_NAME="iphoneos"
elif [ "$PLATFORM_NAME" = "iphonesimulator" ]; then
  PLATFORM_SUFFIX="-iphonesimulator"
  SDK_NAME="iphonesimulator"
elif [ "$PLATFORM_NAME" = "macosx" ]; then
  PLATFORM_SUFFIX=""
  SDK_NAME="macosx"
else
  echo "Error: Unsupported platform '$PLATFORM_NAME'" >&2
  exit 1
fi

# Get the SDK path. This is crucial for cross-compilation.
SDK_PATH=$(xcrun --sdk ${SDK_NAME} --show-sdk-path)
echo "Configured for cross-compilation with SDK: ${SDK_PATH}"

# Export variables needed by crates like `bindgen` to find the correct headers.
export BINDGEN_EXTRA_CLANG_ARGS="--sysroot=${SDK_PATH}"
export SDKROOT="${SDK_PATH}" # Also respected by some build scripts.

STATIC_LIB_NAME="lib$(echo $CRATE_NAME | sed 's/-/_/g').a"
LIPO_INPUT_FILES=""

# Determine if this is a release or debug build.
if [ "$CONFIGURATION" = "Release" ]; then
  RELEASE_FLAG="--release"
  RUST_BUILD_SUBDIR="release"
else
  RELEASE_FLAG=""
  RUST_BUILD_SUBDIR="debug"
fi

# Loop through each architecture specified by Xcode.
for ARCH in $ARCHS; do
  # Determine the Rust target triple based on the architecture and platform.
  if [ "$PLATFORM_NAME" = "macosx" ]; then
    if [ "$ARCH" = "arm64" ]; then
      RUST_TARGET="aarch64-apple-darwin"
    else
      RUST_TARGET="x86_64-apple-darwin"
    fi
  elif [ "$PLATFORM_NAME" = "iphoneos" ]; then
    RUST_TARGET="aarch64-apple-ios"
  elif [ "$PLATFORM_NAME" = "iphonesimulator" ]; then
    if [ "$ARCH" = "arm64" ]; then
      RUST_TARGET="aarch64-apple-ios-sim"
    else
      RUST_TARGET="x86_64-apple-ios"
    fi
  fi

  echo "Building for arch: ${ARCH}, Rust target: ${RUST_TARGET}"

  # --- Configure Linker for Cargo ---
  # Use RUSTFLAGS to pass linker arguments directly to rustc. This is the most
  # reliable way to ensure the linker finds system libraries in the correct SDK.
  export RUSTFLAGS="-C link-arg=-L${SDK_PATH}/usr/lib"
  # export PATH="${SDK_PATH}:$PATH"

  # Run the cargo build command. It will inherit the exported RUSTFLAGS.
  (cd "$RUST_PROJECT_PATH" && ${CARGO} build ${RELEASE_FLAG} --target ${RUST_TARGET})

  BUILT_LIB_PATH="${RUST_PROJECT_PATH}/../target/${RUST_TARGET}/${RUST_BUILD_SUBDIR}/${STATIC_LIB_NAME}"

  # Add the path of the built library to our list for `lipo`.
  LIPO_INPUT_FILES="${LIPO_INPUT_FILES} ${BUILT_LIB_PATH}"
done

# --- Universal Library Creation ---

# Construct the correct, platform-specific destination directory.
DESTINATION_DIR="${BUILT_PRODUCTS_DIR}"
mkdir -p "${DESTINATION_DIR}"
DESTINATION_PATH="${DESTINATION_DIR}/${STATIC_LIB_NAME}"

echo "Creating universal library for architectures: $ARCHS"
echo "Input files: ${LIPO_INPUT_FILES}"
echo "Output path: ${DESTINATION_PATH}"

# Use `lipo` to combine the individual architecture libraries into one universal library.
lipo -create ${LIPO_INPUT_FILES} -output "${DESTINATION_PATH}"

echo "Universal library created successfully."

# Verify the architectures in the final library.
lipo -info "${DESTINATION_PATH}"

echo "Rust build script finished successfully."
