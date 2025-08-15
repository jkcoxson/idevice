// Jackson Coxson

#include <idevice++/lockdown.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/usbmuxd.hpp>
#include <iostream>
#include <optional>
#include <plist/plist++.h>

int main() {
    idevice_init_logger(Debug, Disabled, NULL);

    IdeviceFFI::FfiError                         e;
    std::optional<IdeviceFFI::UsbmuxdConnection> u =
        IdeviceFFI::UsbmuxdConnection::default_new(0, e);
    if (!u) {
        std::cerr << "failed to connect to usbmuxd";
        std::cerr << e.message;
        return 1;
    }

    auto devices = u->get_devices(e);
    if (!devices) {
        std::cerr << "failed to get devices from usbmuxd";
        std::cerr << e.message;
        return 1;
    }
    if (devices->empty()) {
        std::cerr << "no devices connected";
        std::cerr << e.message;
        return 1;
    }

    auto& dev  = (*devices)[0];

    auto  udid = dev.get_udid();
    if (!udid) {
        std::cerr << "no udid\n";
        return 1;
    }

    auto id = dev.get_id();
    if (!id) {
        std::cerr << "no id\n";
        return 1;
    }

    IdeviceFFI::UsbmuxdAddr addr = IdeviceFFI::UsbmuxdAddr::default_new();
    auto                    prov =
        IdeviceFFI::Provider::usbmuxd_new(std::move(addr), /*tag*/ 0, *udid, *id, "reeeeeeeee", e);
    if (!prov) {
        std::cerr << "provider failed: " << e.message << "\n";
        return 1;
    }

    auto client = IdeviceFFI::Lockdown::connect(*prov, e);
    if (!client) {
        std::cerr << "lockdown connect failed: " << e.message << "\n";
        return 1;
    }

    auto pf = prov->get_pairing_file(e);
    if (!pf) {
        std::cerr << "failed to get pairing file: " << e.message << "\n";
        return 1;
    }
    client->start_session(*pf, e);

    auto values = client->get_value(NULL, NULL, e);
    if (!values) {
        std::cerr << "get values failed: " << e.message << "\n";
        return 1;
    }
    PList::Dictionary res = PList::Dictionary(*values);
    std::cout << res.ToXml();
}
