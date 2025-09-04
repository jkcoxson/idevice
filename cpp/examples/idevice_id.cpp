// Jackson Coxson

#include <idevice++/usbmuxd.hpp>
#include <iostream>

int main() {
    auto u = IdeviceFFI::UsbmuxdConnection::default_new(0).expect("failed to connect to usbmuxd");
    auto devices = u.get_devices().expect("failed to get devices from usbmuxd");

    for (IdeviceFFI::UsbmuxdDevice& d : devices) {
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
