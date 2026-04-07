// Jackson Coxson

#include <cstddef>
#include <cstdlib>
#include <iostream>
#include <string>

#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/syslog_relay.hpp>
#include <idevice++/usbmuxd.hpp>

using namespace IdeviceFFI;

int main(int argc, char** argv) {
    idevice_init_logger(Debug, Disabled, NULL);

    std::size_t lines = 10;
    if (argc == 2) {
        lines = static_cast<std::size_t>(std::stoul(argv[1]));
    } else if (argc > 2) {
        std::cerr << "Usage:\n"
                  << "  " << argv[0] << " [lines]\n";
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
    const std::string label = "syslog-relay-client";

    auto              provider =
        Provider::usbmuxd_new(std::move(addr), tag, udid.unwrap(), mux_id.unwrap(), label)
            .expect("failed to create provider");

    auto syslog = SyslogRelay::connect_tcp(provider).expect("failed to connect to syslog relay");

    for (std::size_t i = 0; i < lines; ++i) {
        auto message = syslog.next().expect("failed to read syslog message");
        std::cout << message;
    }

    return 0;
}
