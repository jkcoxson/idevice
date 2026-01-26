#!/usr/bin/env python3
import os
import sys
import subprocess
import shutil
import argparse
from pathlib import Path

class IdeviceFfiXCFrameworkBuilder:
    def __init__(self):
        self.project_root = Path(__file__).parent.absolute()
        self.idevice_ffi_dir = self.project_root / "ffi"
        self.build_dir = self.project_root / "build" / "idevice_ffi"
        self.xcframework_dir = self.project_root / "xcframework"

        # Platform configurations - Support iOS, iOS Simulator, tvOS, tvOS Simulator, macOS, Mac Catalyst
        self.platforms = {
            'iphoneos': {
                'rust_target': 'aarch64-apple-ios',
                'sdk': 'iphoneos',
                'arch': 'arm64',
                'min_version': '12.0',
                'platform_name': 'iOS'
            },
            'iphonesimulator': {
                'rust_targets': ['aarch64-apple-ios-sim', 'x86_64-apple-ios'],
                'sdk': 'iphonesimulator',
                'archs': ['arm64', 'x86_64'],
                'min_version': '12.0',
                'platform_name': 'iOS Simulator'
            },
            'appletvos': {
                'rust_target': 'aarch64-apple-tvos',
                'sdk': 'appletvos',
                'arch': 'arm64',
                'min_version': '12.0',
                'platform_name': 'tvOS'
            },
            'appletvsimulator': {
                'rust_targets': ['aarch64-apple-tvos-sim', 'x86_64-apple-tvos'],
                'sdk': 'appletvsimulator',
                'archs': ['arm64', 'x86_64'],
                'min_version': '12.0',
                'platform_name': 'tvOS Simulator'
            },
            'macosx': {
                'rust_targets': ['aarch64-apple-darwin', 'x86_64-apple-darwin'],
                'sdk': 'macosx',
                'archs': ['arm64', 'x86_64'],
                'min_version': '12.0',
                'platform_name': 'macOS'
            },
            'maccatalyst': {
                'rust_targets': ['aarch64-apple-ios-macabi', 'x86_64-apple-ios-macabi'],
                'sdk': 'macosx',
                'archs': ['arm64', 'x86_64'],
                'min_version': '13.1',
                'platform_name': 'Mac Catalyst'
            }
        }

    def run_command(self, cmd, cwd=None, env=None):
        """Execute a shell command and return the result"""
        print(f"Running: {' '.join(cmd) if isinstance(cmd, list) else cmd}")
        try:
            result = subprocess.run(
                cmd,
                shell=isinstance(cmd, str),
                cwd=cwd or self.idevice_ffi_dir,
                env=env,
                check=True,
                capture_output=True,
                text=True
            )
            return result
        except subprocess.CalledProcessError as e:
            print(f"Command failed: {e}")
            print(f"stdout: {e.stdout}")
            print(f"stderr: {e.stderr}")
            raise

    def prepare_build_environment(self):
        """Prepare the build environment"""
        # Verify idevice ffi directory exists
        if not self.idevice_ffi_dir.exists():
            raise FileNotFoundError(f"idevice ffi directory not found at: {self.idevice_ffi_dir}")

        # Clean previous builds
        if self.build_dir.exists():
            shutil.rmtree(self.build_dir)

        # Only clean specific idevice_ffi XCFramework, not the entire xcframework directory
        self.xcframework_dir.mkdir(parents=True, exist_ok=True)
        idevice_ffi_xcframework = self.xcframework_dir / "idevice_ffi.xcframework"
        if idevice_ffi_xcframework.exists():
            shutil.rmtree(idevice_ffi_xcframework)

        self.build_dir.mkdir(parents=True, exist_ok=True)

        # Install rust-src component for nightly (required for -Zbuild-std)
        try:
            home_dir = os.environ.get('HOME', os.path.expanduser('~'))
            env = {
                'PATH': f"{home_dir}/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:/usr/local/bin",
                'HOME': home_dir,
                'USER': os.environ.get('USER', os.getlogin()),
                'SHELL': os.environ.get('SHELL', '/bin/zsh'),
                'TERM': os.environ.get('TERM', 'xterm-256color'),
                'LANG': os.environ.get('LANG', 'en_US.UTF-8'),
                'LC_ALL': os.environ.get('LC_ALL', 'en_US.UTF-8')
            }
            self.run_command(['rustup', 'component', 'add', 'rust-src', '--toolchain', 'nightly'], cwd=self.project_root, env=env)
        except subprocess.CalledProcessError:
            print("rust-src component might already be installed")

        # Ensure stable targets are installed where available
        try:
            home_dir = os.environ.get('HOME', os.path.expanduser('~'))
            env = {
                'PATH': f"{home_dir}/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:/usr/local/bin",
                'HOME': home_dir,
                'USER': os.environ.get('USER', os.getlogin()),
                'SHELL': os.environ.get('SHELL', '/bin/zsh'),
                'TERM': os.environ.get('TERM', 'xterm-256color'),
                'LANG': os.environ.get('LANG', 'en_US.UTF-8'),
                'LC_ALL': os.environ.get('LC_ALL', 'en_US.UTF-8')
            }
            for target in [
                'aarch64-apple-ios', 'aarch64-apple-ios-sim', 'x86_64-apple-ios',
                'aarch64-apple-darwin', 'x86_64-apple-darwin',
                'aarch64-apple-ios-macabi', 'x86_64-apple-ios-macabi'
            ]:
                try:
                    self.run_command(['rustup', 'target', 'add', target], cwd=self.project_root, env=env)
                except subprocess.CalledProcessError:
                    print(f"Target {target} might already be installed or not required on this toolchain")
        except Exception as e:
            print(f"Warning: failed to ensure some targets: {e}")

        # Note: tvOS targets are Tier 3 and not available as pre-built targets
        # We'll use -Zbuild-std to build them from source
        print("Using nightly Rust with -Zbuild-std for tvOS targets (Tier 3)")

    def build_for_target(self, platform, rust_target, arch):
        """Build the Rust crate for a specific target"""
        platform_config = self.platforms[platform]

        # Create build directory for this target
        build_subdir = self.build_dir / f"{platform}-{arch}"
        build_subdir.mkdir(parents=True, exist_ok=True)

        # Set up clean environment for cross-compilation
        home_dir = os.environ.get('HOME', os.path.expanduser('~'))
        env = {
            'PATH': f"{home_dir}/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:/usr/local/bin",
            'HOME': home_dir,
            'USER': os.environ.get('USER', os.getlogin()),
            'SHELL': os.environ.get('SHELL', '/bin/zsh'),
            'TERM': os.environ.get('TERM', 'xterm-256color'),
            'LANG': os.environ.get('LANG', 'en_US.UTF-8'),
            'LC_ALL': os.environ.get('LC_ALL', 'en_US.UTF-8')
        }

        # Set deployment target
        if platform in ['iphoneos', 'iphonesimulator']:
            env['IPHONEOS_DEPLOYMENT_TARGET'] = platform_config['min_version']
        elif platform in ['appletvos', 'appletvsimulator']:
            env['TVOS_DEPLOYMENT_TARGET'] = platform_config['min_version']
        elif platform in ['macosx']:
            env['MACOSX_DEPLOYMENT_TARGET'] = platform_config['min_version']
        elif platform in ['maccatalyst']:
            env['MACOSX_DEPLOYMENT_TARGET'] = platform_config['min_version']
            env['IPHONEOS_DEPLOYMENT_TARGET'] = platform_config['min_version']

        # Build the static library
        print(f"\n=== Building for {platform} ({arch}) using target {rust_target} ===")

        # Use nightly toolchain for tvOS targets (Tier 3)
        if ('apple-tvos' in rust_target):
            cargo_cmd = [
                'cargo', '+nightly', 'build',
                '-Zbuild-std=std,panic_abort',
                '--release',
                '--target', rust_target,
                '--features', 'full,ring',
                '--no-default-features'
            ]
        else:
            cargo_cmd = [
                'cargo', 'build',
                '--release',
                '--target', rust_target,
                '--features', 'full,ring',
                '--no-default-features'
            ]

        self.run_command(cargo_cmd, cwd=self.idevice_ffi_dir, env=env)

        # Copy the built library
        target_dir = self.idevice_ffi_dir.parent / "target" / rust_target / "release"
        lib_file = target_dir / "libidevice_ffi.a"

        if not lib_file.exists():
            raise FileNotFoundError(f"Built library not found at: {lib_file}")

        output_lib = build_subdir / "libidevice_ffi.a"
        shutil.copy2(lib_file, output_lib)

        # Copy headers
        header_file = self.idevice_ffi_dir / "idevice.h"
        if header_file.exists():
            headers_dir = build_subdir / "include"
            headers_dir.mkdir(exist_ok=True)
            shutil.copy2(header_file, headers_dir / "idevice.h")
        else:
            print(f"Warning: Header file not found at {header_file}")

        return build_subdir

    def fix_tvos_platform_metadata(self, lib_path, platform):
        """Fix the platform metadata in the static library for tvOS"""
        print(f"Fixing platform metadata for {platform}...")

        # Extract all object files from the archive
        temp_dir = lib_path.parent / "temp_objects"
        if temp_dir.exists():
            shutil.rmtree(temp_dir)
        temp_dir.mkdir()

        try:
            # Extract the archive
            self.run_command(['ar', 'x', str(lib_path)], cwd=temp_dir)

            # Get list of object files
            object_files = list(temp_dir.glob("*.o"))

            for obj_file in object_files:
                # Use vtool to change the platform
                try:
                    # Change from iOS (platform 2) to tvOS (platform 3)
                    # Use -replace to modify in place
                    self.run_command(['vtool', '-set-build-version', '3', '12.0', '12.0', '-replace', str(obj_file)], cwd=temp_dir)
                    print(f"Successfully updated platform metadata for {obj_file.name}")
                except subprocess.CalledProcessError as e:
                    print(f"vtool failed for {obj_file.name}: {e}")
                    # Continue with other files even if one fails
                    continue

            # Recreate the archive with modified object files
            ar_cmd = ['ar', 'rcs', str(lib_path)] + [str(obj) for obj in object_files]
            self.run_command(ar_cmd, cwd=temp_dir)

        finally:
            # Clean up
            if temp_dir.exists():
                shutil.rmtree(temp_dir)

    def get_lib_archs(self, lib_path: Path):
        """Return a list of architectures present in a static library using lipo"""
        try:
            result = self.run_command(['lipo', '-info', str(lib_path)], cwd=self.project_root)
            output = result.stdout.strip() or result.stderr.strip()
            archs = []
            if 'are:' in output:
                archs = output.split('are:')[-1].strip().split()
            elif 'architecture:' in output:
                archs = [output.split('architecture:')[-1].strip()]
            return archs
        except Exception:
            return []

    def create_fat_library(self, platform, build_paths):
        """Create a fat library for platforms with multiple architectures"""
        if len(build_paths) == 1:
            return build_paths[0]

        fat_dir = self.build_dir / f"{platform}-fat"
        fat_dir.mkdir(exist_ok=True)

        # Copy headers from first build
        first_build = build_paths[0]
        if (first_build / "include").exists():
            shutil.copytree(first_build / "include", fat_dir / "include", dirs_exist_ok=True)

        # Create fat library
        lib_paths = [build_path / "libidevice_ffi.a" for build_path in build_paths]
        fat_lib = fat_dir / "libidevice_ffi.a"

        lipo_cmd = ['lipo', '-create'] + [str(p) for p in lib_paths] + ['-output', str(fat_lib)]
        self.run_command(lipo_cmd, cwd=self.project_root)

        print(f"Created fat library: {fat_lib}")
        return fat_dir

    def build_platform(self, platform):
        """Build for a specific platform"""
        platform_config = self.platforms[platform]
        build_paths = []

        if 'rust_targets' in platform_config:
            # Multi-architecture platform
            for i, rust_target in enumerate(platform_config['rust_targets']):
                arch = platform_config['archs'][i]
                build_path = self.build_for_target(platform, rust_target, arch)
                build_paths.append(build_path)

            return self.create_fat_library(platform, build_paths)
        else:
            # Single architecture platform
            rust_target = platform_config['rust_target']
            arch = platform_config['arch']
            return self.build_for_target(platform, rust_target, arch)

    def create_xcframework(self, platform_builds):
        """Create the XCFramework manually with correct platform identifiers"""
        xcframework_path = self.xcframework_dir / "idevice_ffi.xcframework"

        # Remove existing XCFramework
        if xcframework_path.exists():
            shutil.rmtree(xcframework_path)

        xcframework_path.mkdir(parents=True)

        print(f"\n=== Creating XCFramework manually ===")

        # Create the library entries for Info.plist
        available_libraries = []

        for platform, build_path in platform_builds.items():
            lib_path = build_path / "libidevice_ffi.a"
            headers_path = build_path / "include"

            if not lib_path.exists():
                continue

            platform_config = self.platforms[platform]

            # Determine the correct platform identifier and supported architectures
            if platform == 'appletvos':
                library_identifier = "tvos-arm64"
                supported_platform = "tvos"
                supported_architectures = ["arm64"]
                platform_variant = None
            elif platform == 'appletvsimulator':
                detected_archs = self.get_lib_archs(lib_path) or platform_config.get('archs', [])
                sorted_archs = sorted(detected_archs)
                if sorted_archs == ["arm64", "x86_64"]:
                    library_identifier = "tvos-arm64_x86_64-simulator"
                elif sorted_archs == ["arm64"]:
                    library_identifier = "tvos-arm64-simulator"
                elif sorted_archs == ["x86_64"]:
                    library_identifier = "tvos-x86_64-simulator"
                else:
                    library_identifier = "tvos-simulator"
                supported_platform = "tvos"
                supported_architectures = sorted_archs
                platform_variant = "simulator"
            elif platform == 'iphoneos':
                library_identifier = "ios-arm64"
                supported_platform = "ios"
                supported_architectures = ["arm64"]
                platform_variant = None
            elif platform == 'iphonesimulator':
                detected_archs = self.get_lib_archs(lib_path) or platform_config.get('archs', [])
                sorted_archs = sorted(detected_archs)
                if sorted_archs == ["arm64", "x86_64"]:
                    library_identifier = "ios-arm64_x86_64-simulator"
                elif sorted_archs == ["arm64"]:
                    library_identifier = "ios-arm64-simulator"
                elif sorted_archs == ["x86_64"]:
                    library_identifier = "ios-x86_64-simulator"
                else:
                    library_identifier = "ios-simulator"
                supported_platform = "ios"
                supported_architectures = sorted_archs
                platform_variant = "simulator"
            elif platform == 'macosx':
                detected_archs = self.get_lib_archs(lib_path) or platform_config.get('archs', [])
                sorted_archs = sorted(detected_archs)
                if sorted_archs == ["arm64", "x86_64"]:
                    library_identifier = "macos-arm64_x86_64"
                elif sorted_archs == ["arm64"]:
                    library_identifier = "macos-arm64"
                elif sorted_archs == ["x86_64"]:
                    library_identifier = "macos-x86_64"
                else:
                    library_identifier = "macos"
                supported_platform = "macos"
                supported_architectures = sorted_archs
                platform_variant = None
            elif platform == 'maccatalyst':
                detected_archs = self.get_lib_archs(lib_path) or platform_config.get('archs', [])
                sorted_archs = sorted(detected_archs)
                if sorted_archs == ["arm64", "x86_64"]:
                    library_identifier = "ios-arm64_x86_64-maccatalyst"
                elif sorted_archs == ["arm64"]:
                    library_identifier = "ios-arm64-maccatalyst"
                elif sorted_archs == ["x86_64"]:
                    library_identifier = "ios-x86_64-maccatalyst"
                else:
                    library_identifier = "ios-maccatalyst"
                supported_platform = "ios"
                supported_architectures = sorted_archs
                platform_variant = "maccatalyst"
            else:
                continue  # Skip unknown platforms

            # Create platform-specific directory
            platform_dir = xcframework_path / library_identifier
            platform_dir.mkdir()

            # Copy library
            shutil.copy2(lib_path, platform_dir / "libidevice_ffi.a")

            # Copy headers
            if headers_path.exists():
                headers_dir = platform_dir / "Headers"
                shutil.copytree(headers_path, headers_dir)

            # Create library entry for Info.plist
            library_entry = {
                "BinaryPath": "libidevice_ffi.a",
                "HeadersPath": "Headers",
                "LibraryIdentifier": library_identifier,
                "LibraryPath": "libidevice_ffi.a",
                "SupportedArchitectures": supported_architectures,
                "SupportedPlatform": supported_platform
            }

            if platform_variant:
                library_entry["SupportedPlatformVariant"] = platform_variant

            available_libraries.append(library_entry)

        # Create Info.plist
        info_plist_content = {
            "AvailableLibraries": available_libraries,
            "CFBundlePackageType": "XFWK",
            "XCFrameworkFormatVersion": "1.0"
        }

        # Write Info.plist
        import plistlib
        info_plist_path = xcframework_path / "Info.plist"
        with open(info_plist_path, 'wb') as f:
            plistlib.dump(info_plist_content, f)

        print(f"\n✅ XCFramework created successfully:")
        print(f"   idevice_ffi: {xcframework_path}")
        return xcframework_path

    def build_all_platforms(self):
        """Build for all platforms"""
        platform_builds = {}

        for platform in self.platforms.keys():
            print(f"\n🚀 Building for {self.platforms[platform]['platform_name']} ({platform})")
            build_path = self.build_platform(platform)
            platform_builds[platform] = build_path

        return self.create_xcframework(platform_builds)

    def build_active_platform(self, sdk_name):
        """Build for the active platform from Xcode environment"""
        platform_map = {
            'iphoneos': 'iphoneos',
            'iphonesimulator': 'iphonesimulator',
            'appletvos': 'appletvos',
            'appletvsimulator': 'appletvsimulator',
            'macosx': 'macosx',
            'maccatalyst': 'maccatalyst'
        }

        platform = platform_map.get(sdk_name.lower())
        if not platform:
            raise ValueError(f"Unsupported SDK: {sdk_name}")

        print(f"🚀 Building for active platform: {self.platforms[platform]['platform_name']}")
        build_path = self.build_platform(platform)

        # Create a single-platform XCFramework
        return self.create_xcframework({platform: build_path})

def main():
    parser = argparse.ArgumentParser(description='Build idevice FFI XCFramework')
    parser.add_argument('--sdk', help='Build for specific SDK (from Xcode environment)')
    args = parser.parse_args()

    builder = IdeviceFfiXCFrameworkBuilder()

    try:
        print("🔧 Preparing build environment...")
        builder.prepare_build_environment()

        if args.sdk:
            builder.build_active_platform(args.sdk)
        else:
            builder.build_all_platforms()

    except Exception as e:
        print(f"❌ Build failed: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main()
