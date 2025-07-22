// Jackson Coxson

#include "ffi.hpp"
#include "usbmuxd.hpp"
#include <iostream>
#include <optional>

int main() {
    std::cout << "Getting devices from usbmuxd\n";

    IdeviceFFI::FfiError                         e;
    std::optional<IdeviceFFI::UsbmuxdConnection> u =
        IdeviceFFI::UsbmuxdConnection::default_new(0, e);
    if (u == std::nullopt) {
        std::cerr << "failed to connect to usbmuxd";
        std::cerr << e.message;
    }

    auto devices = u->get_devices(e);
    if (u == std::nullopt) {
        std::cerr << "failed to get devices from usbmuxd";
        std::cerr << e.message;
    }

    for (IdeviceFFI::UsbmuxdDevice& d : *devices) {
        auto udid = d.get_udid();
        if (!udid) {
            std::cerr << "failed to get udid";
            continue;
        }
        auto connection_type = d.get_connection_type();
        if (!connection_type) {
            std::cerr << "failed to get connection type";
            continue;
        }
        std::cout << *udid << " (" << connection_type->to_string() << ")" << "\n";
    }
}
