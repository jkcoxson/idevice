#include <fstream>
#include <iomanip>
#include <iostream>
#include <string>
#include <vector>

// Idevice++ library headers
#include <idevice++/lockdown.hpp>
#include <idevice++/mobile_image_mounter.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/usbmuxd.hpp>
#include <plist/plist++.h>

// --- Helper Functions ---

/**
 * @brief Reads an entire file into a byte vector.
 * @param path The path to the file.
 * @return A vector containing the file's data.
 */
std::vector<uint8_t> read_file(const std::string& path) {
    std::ifstream file(path, std::ios::binary | std::ios::ate);
    if (!file) {
        throw std::runtime_error("Failed to open file: " + path);
    }
    std::streamsize size = file.tellg();
    file.seekg(0, std::ios::beg);
    std::vector<uint8_t> buffer(size);
    if (!file.read(reinterpret_cast<char*>(buffer.data()), size)) {
        throw std::runtime_error("Failed to read file: " + path);
    }
    return buffer;
}

/**
 * @brief Prints the command usage instructions.
 */
void print_usage(const char* prog_name) {
    std::cerr << "Usage: " << prog_name << " [options] <subcommand>\n\n"
              << "A tool to manage developer images on a device.\n\n"
              << "Options:\n"
              << "  --udid <UDID>    Target a specific device by its UDID.\n\n"
              << "Subcommands:\n"
              << "  list                                 List mounted images.\n"
              << "  unmount                              Unmount the developer image.\n"
              << "  mount [mount_options]                Mount a developer image.\n\n"
              << "Mount Options:\n"
              << "  --image <path>       (Required) Path to the DeveloperDiskImage.dmg.\n"
              << "  --signature <path>   (Required for iOS < 17) Path to the .signature file.\n"
              << "  --manifest <path>    (Required for iOS 17+) Path to the BuildManifest.plist.\n"
              << "  --trustcache <path>  (Required for iOS 17+) Path to the trust cache file.\n"
              << std::endl;
}

// --- Main Logic ---

int main(int argc, char** argv) {
    idevice_init_logger(Debug, Disabled, NULL);
    // --- 1. Argument Parsing ---
    if (argc < 2) {
        print_usage(argv[0]);
        return 1;
    }

    std::string udid_arg;
    std::string subcommand;
    std::string image_path;
    std::string signature_path;
    std::string manifest_path;
    std::string trustcache_path;

    for (int i = 1; i < argc; ++i) {
        std::string arg = argv[i];
        if (arg == "--udid" && i + 1 < argc) {
            udid_arg = argv[++i];
        } else if (arg == "--image" && i + 1 < argc) {
            image_path = argv[++i];
        } else if (arg == "--signature" && i + 1 < argc) {
            signature_path = argv[++i];
        } else if (arg == "--manifest" && i + 1 < argc) {
            manifest_path = argv[++i];
        } else if (arg == "--trustcache" && i + 1 < argc) {
            trustcache_path = argv[++i];
        } else if (arg == "list" || arg == "mount" || arg == "unmount") {
            subcommand = arg;
        } else if (arg == "--help" || arg == "-h") {
            print_usage(argv[0]);
            return 0;
        }
    }

    if (subcommand.empty()) {
        std::cerr << "Error: No subcommand specified. Use 'list', 'mount', or 'unmount'."
                  << std::endl;
        print_usage(argv[0]);
        return 1;
    }

    try {
        // --- 2. Device Connection ---
        auto u =
            IdeviceFFI::UsbmuxdConnection::default_new(0).expect("Failed to connect to usbmuxd");
        auto devices = u.get_devices().expect("Failed to get devices from usbmuxd");
        if (devices.empty()) {
            throw std::runtime_error("No devices connected.");
        }

        IdeviceFFI::UsbmuxdDevice* target_dev = nullptr;
        if (!udid_arg.empty()) {
            for (auto& dev : devices) {
                if (dev.get_udid().unwrap_or("") == udid_arg) {
                    target_dev = &dev;
                    break;
                }
            }
            if (!target_dev) {
                throw std::runtime_error("Device with UDID " + udid_arg + " not found.");
            }
        } else {
            target_dev = &devices[0]; // Default to the first device
        }

        auto                    udid = target_dev->get_udid().expect("Device has no UDID");
        auto                    id   = target_dev->get_id().expect("Device has no ID");

        IdeviceFFI::UsbmuxdAddr addr = IdeviceFFI::UsbmuxdAddr::default_new();
        auto prov = IdeviceFFI::Provider::usbmuxd_new(std::move(addr), 0, udid, id, "mounter-tool")
                        .expect("Failed to create provider");

        // --- 3. Connect to Lockdown & Get iOS Version ---
        auto lockdown_client =
            IdeviceFFI::Lockdown::connect(prov).expect("Lockdown connect failed");

        auto pairing_file = prov.get_pairing_file().expect("Failed to get pairing file");
        lockdown_client.start_session(pairing_file).expect("Failed to start session");

        auto version_plist = lockdown_client.get_value("ProductVersion", NULL)
                                 .expect("Failed to get ProductVersion");
        PList::String version_node(version_plist);
        std::string   version_str = version_node.GetValue();
        std::cout << "Version string: " << version_str << std::endl;

        if (version_str.empty()) {
            throw std::runtime_error(
                "Failed to get a valid ProductVersion string from the device.");
        }
        int  major_version  = std::stoi(version_str);

        // --- 4. Connect to MobileImageMounter ---
        auto mounter_client = IdeviceFFI::MobileImageMounter::connect(prov).expect(
            "Failed to connect to image mounter");

        // --- 5. Execute Subcommand ---
        if (subcommand == "list") {
            auto images = mounter_client.copy_devices().expect("Failed to get images");
            std::cout << "Mounted Images:\n";
            for (plist_t p : images) {
                PList::Dictionary dict(p);
                std::cout << dict.ToXml() << std::endl;
            }

        } else if (subcommand == "unmount") {
            const char* unmount_path = (major_version < 17) ? "/Developer" : "/System/Developer";
            mounter_client.unmount_image(unmount_path).expect("Failed to unmount image");
            std::cout << "Successfully unmounted image from " << unmount_path << std::endl;

        } else if (subcommand == "mount") {
            if (image_path.empty()) {
                throw std::runtime_error("Mount command requires --image <path>");
            }
            auto image_data = read_file(image_path);

            if (major_version < 17) {
                if (signature_path.empty()) {
                    throw std::runtime_error("iOS < 17 requires --signature <path>");
                }
                auto signature_data = read_file(signature_path);
                mounter_client
                    .mount_developer(image_data.data(),
                                     image_data.size(),
                                     signature_data.data(),
                                     signature_data.size())
                    .expect("Failed to mount developer image");
            } else { // iOS 17+
                if (manifest_path.empty() || trustcache_path.empty()) {
                    throw std::runtime_error("iOS 17+ requires --manifest and --trustcache paths");
                }
                auto manifest_data   = read_file(manifest_path);
                auto trustcache_data = read_file(trustcache_path);

                auto chip_id_plist   = lockdown_client.get_value(nullptr, "UniqueChipID")
                                         .expect("Failed to get UniqueChipID");
                PList::Integer                      chip_id_node(chip_id_plist);
                uint64_t                            unique_chip_id    = chip_id_node.GetValue();

                std::function<void(size_t, size_t)> progress_callback = [](size_t n, size_t d) {
                    if (d == 0) {
                        return;
                    }
                    double percent = (static_cast<double>(n) / d) * 100.0;
                    std::cout << "\rProgress: " << std::fixed << std::setprecision(2) << percent
                              << "%" << std::flush;
                    if (n == d) {
                        std::cout << std::endl;
                    }
                };

                mounter_client
                    .mount_personalized_with_callback(prov,
                                                      image_data.data(),
                                                      image_data.size(),
                                                      trustcache_data.data(),
                                                      trustcache_data.size(),
                                                      manifest_data.data(),
                                                      manifest_data.size(),
                                                      nullptr, // info_plist
                                                      unique_chip_id,
                                                      progress_callback)
                    .expect("Failed to mount personalized image");
            }
            std::cout << "Successfully mounted image." << std::endl;
        }

    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << std::endl;
        return 1;
    }

    return 0;
}
