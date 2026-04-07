// Jackson Coxson

#include <fstream>
#include <iostream>
#include <string>
#include <vector>

#include <idevice++/afc.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/usbmuxd.hpp>

using namespace IdeviceFFI;

int main(int argc, char** argv) {
    idevice_init_logger(Debug, Disabled, NULL);

    // Usage:
    //   afc list <path>
    //   afc mkdir <path>
    //   afc download <device_path> <host_path>
    if (argc < 3) {
        std::cerr << "Usage:\n"
                  << "  " << argv[0] << " list <path>\n"
                  << "  " << argv[0] << " mkdir <path>\n"
                  << "  " << argv[0] << " download <device_path> <host_path>\n";
        return 2;
    }

    std::string command = argv[1];

    auto mux = UsbmuxdConnection::default_new(0).expect("failed to connect to usbmuxd");

    auto devices = mux.get_devices().expect("failed to list devices");
    if (devices.empty()) {
        std::cerr << "no devices connected\n";
        return 1;
    }

    auto& dev = devices[0];

    auto udid = dev.get_udid();
    if (udid.is_none()) {
        std::cerr << "device has no UDID\n";
        return 1;
    }

    auto mux_id = dev.get_id();
    if (mux_id.is_none()) {
        std::cerr << "device has no mux id\n";
        return 1;
    }

    auto              addr  = UsbmuxdAddr::default_new();
    const uint32_t    tag   = 0;
    const std::string label = "afc-client";

    auto              provider =
        Provider::usbmuxd_new(std::move(addr), tag, udid.unwrap(), mux_id.unwrap(), label)
            .expect("failed to create provider");

    auto afc = AfcClient::connect(provider).expect("failed to connect to AFC service");

    if (command == "list") {
        if (argc != 3) {
            std::cerr << "Usage:\n"
                      << "  " << argv[0] << " list <path>\n";
            return 2;
        }

        auto entries = afc.list_directory(argv[2]).expect("failed to list directory");
        for (const auto& entry : entries) {
            std::cout << entry << "\n";
        }
        return 0;
    }

    if (command == "mkdir") {
        if (argc != 3) {
            std::cerr << "Usage:\n"
                      << "  " << argv[0] << " mkdir <path>\n";
            return 2;
        }

        afc.make_directory(argv[2]).expect("failed to create directory");
        std::cout << "Directory created: " << argv[2] << "\n";
        return 0;
    }

    if (command == "download") {
        if (argc != 4) {
            std::cerr << "Usage:\n"
                      << "  " << argv[0] << " download <device_path> <host_path>\n";
            return 2;
        }

        auto file = afc.file_open(argv[2], AfcRdOnly).expect("failed to open device file");
        auto data = file.read_all().expect("failed to read device file");
        file.close().expect("failed to close device file");

        std::ofstream out(argv[3], std::ios::binary);
        if (!out.is_open()) {
            std::cerr << "failed to open output file: " << argv[3] << "\n";
            return 1;
        }

        out.write(reinterpret_cast<const char*>(data.data()),
                  static_cast<std::streamsize>(data.size()));
        out.close();

        std::cout << "Downloaded " << argv[2] << " to " << argv[3] << " (" << data.size()
                  << " bytes)\n";
        return 0;
    }

    std::cerr << "Unknown command: " << command << "\n";
    return 2;
}
