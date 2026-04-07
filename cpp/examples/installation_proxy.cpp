// Jackson Coxson

#include <iostream>
#include <string>
#include <vector>

#include <idevice++/ffi.hpp>
#include <idevice++/installation_proxy.hpp>
#include <idevice++/option.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/usbmuxd.hpp>
#include <plist/plist++.h>

using namespace IdeviceFFI;

int main() {
    idevice_init_logger(Debug, Disabled, NULL);

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
    const std::string label = "installation-proxy-client";

    auto              provider =
        Provider::usbmuxd_new(std::move(addr), tag, udid.unwrap(), mux_id.unwrap(), label)
            .expect("failed to create provider");

    auto installation_proxy = InstallationProxy::connect(provider)
                                  .expect("failed to connect to installation proxy");

    auto apps               = installation_proxy.browse(None).expect("failed to browse apps");

    std::cout << "Found " << apps.size() << " apps\n";
    for (auto app : apps) {
        PList::Dictionary dict(app);
        std::cout << dict.ToXml() << "\n";
        plist_free(app);
    }

    return 0;
}
