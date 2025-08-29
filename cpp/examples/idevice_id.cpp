// Jackson Coxson

#include <idevice++/usbmuxd.hpp>
#include <iostream>

int main() {
    auto u = IdeviceFFI::UsbmuxdConnection::default_new(0);
    if_let_err(u, e, {
        std::cerr << "failed to connect to usbmuxd";
        std::cerr << e.message;
    });

    auto devices = u.unwrap().get_devices();
    if_let_err(devices, e, {
        std::cerr << "failed to get devices from usbmuxd";
        std::cerr << e.message;
    });

    for (IdeviceFFI::UsbmuxdDevice& d : devices.unwrap()) {
        auto udid = d.get_udid();
        if (udid.is_none()) {
            std::cerr << "failed to get udid";
            continue;
        }
        auto connection_type = d.get_connection_type();
        if (connection_type.is_none()) {
            std::cerr << "failed to get connection type";
            continue;
        }
        std::cout << udid.unwrap() << " (" << connection_type.unwrap().to_string() << ")" << "\n";
    }
}
