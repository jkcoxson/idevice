// Jackson Coxson

#include <idevice++/lockdown.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/usbmuxd.hpp>
#include <iostream>
#include <plist/plist++.h>

int main() {
    idevice_init_logger(Debug, Disabled, NULL);

    auto u = IdeviceFFI::UsbmuxdConnection::default_new(0).expect("failed to connect to usbmuxd");

    auto devices = u.get_devices().expect("failed to get devices from usbmuxd");

    if (devices.empty()) {
        std::cerr << "no devices connected";
        return 1;
    }

    auto& dev  = (devices)[0];

    auto  udid = dev.get_udid();
    if (udid.is_none()) {
        std::cerr << "no udid\n";
        return 1;
    }

    auto id = dev.get_id();
    if (id.is_none()) {
        std::cerr << "no id\n";
        return 1;
    }

    IdeviceFFI::UsbmuxdAddr addr = IdeviceFFI::UsbmuxdAddr::default_new();
    auto                    prov = IdeviceFFI::Provider::usbmuxd_new(
                    std::move(addr), /*tag*/ 0, udid.unwrap(), id.unwrap(), "reeeeeeeee")
                    .expect("Failed to create usbmuxd provider");

    auto client = IdeviceFFI::Lockdown::connect(prov).expect("lockdown connect failed");

    auto pf     = prov.get_pairing_file().expect("failed to get pairing file");
    client.start_session(pf).expect("failed to start session");

    auto values = client.get_value(NULL, NULL);
    match_result(
        values,
        ok_val,
        {
            PList::Dictionary res = PList::Dictionary(ok_val);
            std::cout << res.ToXml();
        },
        e,
        {
            std::cerr << "get values failed: " << e.message << "\n";
            return 1;
        });
}
