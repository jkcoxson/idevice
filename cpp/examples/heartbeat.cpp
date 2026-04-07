// Jackson Coxson

#include <cstdlib>
#include <iostream>
#include <string>

#include <idevice++/ffi.hpp>
#include <idevice++/heartbeat.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/usbmuxd.hpp>

using namespace IdeviceFFI;

int main(int argc, char** argv) {
    idevice_init_logger(Debug, Disabled, NULL);

    // Usage:
    //   heartbeat [iterations]
    size_t iterations = 3;
    if (argc == 2) {
        iterations = static_cast<size_t>(std::stoul(argv[1]));
    } else if (argc > 2) {
        std::cerr << "Usage:\n"
                  << "  " << argv[0] << " [iterations]\n";
        return 2;
    }

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
    const std::string label = "heartbeat-client";

    auto              provider =
        Provider::usbmuxd_new(std::move(addr), tag, udid.unwrap(), mux_id.unwrap(), label)
            .expect("failed to create provider");

    auto heartbeat = Heartbeat::connect(provider).expect("failed to connect to heartbeat service");

    u_int64_t current_interval = 15;
    for (size_t i = 0; i < iterations; ++i) {
        auto new_interval = heartbeat.get_marco(current_interval).expect("failed to get marco");
        std::cout << "Marco interval: " << new_interval << "\n";

        heartbeat.send_polo().expect("failed to send polo");
        current_interval = new_interval + 5;
    }

    return 0;
}
