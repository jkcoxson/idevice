// Jackson Coxson

#include <fstream>
#include <iostream>
#include <string>

#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/springboardservices.hpp>
#include <idevice++/usbmuxd.hpp>

using namespace IdeviceFFI;

int main(int argc, char** argv) {
    idevice_init_logger(Debug, Disabled, NULL);

    // Usage:
    //   springboardservices orientation
    //   springboardservices icon <bundle_id> <output.png>
    //   springboardservices wallpaper homescreen <output.png>
    //   springboardservices wallpaper lockscreen <output.png>
    if (argc < 2) {
        std::cerr << "Usage:\n"
                  << "  " << argv[0] << " orientation\n"
                  << "  " << argv[0] << " icon <bundle_id> <output.png>\n"
                  << "  " << argv[0] << " wallpaper homescreen <output.png>\n"
                  << "  " << argv[0] << " wallpaper lockscreen <output.png>\n";
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
    const std::string label = "springboardservices-client";

    auto              provider =
        Provider::usbmuxd_new(std::move(addr), tag, udid.unwrap(), mux_id.unwrap(), label)
            .expect("failed to create provider");

    auto springboard =
        SpringBoardServices::connect(provider).expect("failed to connect to springboardservices");

    if (command == "orientation") {
        auto orientation = springboard.get_interface_orientation().expect("failed to get orientation");
        std::cout << static_cast<int>(orientation) << "\n";
        return 0;
    }

    if (command == "icon") {
        if (argc != 4) {
            std::cerr << "Usage:\n"
                      << "  " << argv[0] << " icon <bundle_id> <output.png>\n";
            return 2;
        }

        std::string bundle_id = argv[2];
        std::string out_path  = argv[3];

        auto icon = springboard.get_icon(bundle_id).expect("failed to get icon");

        std::ofstream out(out_path, std::ios::binary);
        if (!out.is_open()) {
            std::cerr << "failed to open output file: " << out_path << "\n";
            return 1;
        }

        out.write(reinterpret_cast<const char*>(icon.data()),
                  static_cast<std::streamsize>(icon.size()));
        out.close();

        std::cout << "Icon saved to " << out_path << " (" << icon.size() << " bytes)\n";
        return 0;
    }

    if (command == "wallpaper") {
        if (argc != 4) {
            std::cerr << "Usage:\n"
                      << "  " << argv[0] << " wallpaper homescreen <output.png>\n"
                      << "  " << argv[0] << " wallpaper lockscreen <output.png>\n";
            return 2;
        }

        std::string wallpaper_type = argv[2];
        std::string out_path       = argv[3];

        std::vector<uint8_t> wallpaper;
        if (wallpaper_type == "homescreen") {
            wallpaper = springboard.get_home_screen_wallpaper_preview()
                            .expect("failed to get homescreen wallpaper preview");
        } else if (wallpaper_type == "lockscreen") {
            wallpaper = springboard.get_lock_screen_wallpaper_preview()
                            .expect("failed to get lockscreen wallpaper preview");
        } else {
            std::cerr << "Invalid wallpaper type: " << wallpaper_type << "\n";
            return 2;
        }

        std::ofstream out(out_path, std::ios::binary);
        if (!out.is_open()) {
            std::cerr << "failed to open output file: " << out_path << "\n";
            return 1;
        }

        out.write(reinterpret_cast<const char*>(wallpaper.data()),
                  static_cast<std::streamsize>(wallpaper.size()));
        out.close();

        std::cout << "Wallpaper saved to " << out_path << " (" << wallpaper.size()
                  << " bytes)\n";
        return 0;
    }

    std::cerr << "Unknown command: " << command << "\n";
    return 2;
}
