// Jackson Coxson

#include <fstream>
#include <iostream>
#include <string>

#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/screenshotr.hpp>
#include <idevice++/usbmuxd.hpp>

using namespace IdeviceFFI;

int main(int argc, char** argv) {
    idevice_init_logger(Debug, Disabled, NULL);

    // Usage:
    //   screenshotr <output.png>
    if (argc != 2) {
        std::cerr << "Usage:\n"
                  << "  " << argv[0] << " <output.png>\n";
        return 2;
    }

    std::string out_path = argv[1];

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
    const std::string label = "screenshotr-client";

    auto              provider =
        Provider::usbmuxd_new(std::move(addr), tag, udid.unwrap(), mux_id.unwrap(), label)
            .expect("failed to create provider");

    auto screenshotr =
        Screenshotr::connect(provider).expect("failed to connect to screenshotr service");

    auto buf = screenshotr.take_screenshot().expect("failed to capture screenshot");

    std::ofstream out(out_path, std::ios::binary);
    if (!out.is_open()) {
        std::cerr << "failed to open output file: " << out_path << "\n";
        return 1;
    }

    out.write(reinterpret_cast<const char*>(buf.data()), static_cast<std::streamsize>(buf.size()));
    out.close();

    std::cout << "Screenshot saved to " << out_path << " (" << buf.size() << " bytes)\n";
    return 0;
}
