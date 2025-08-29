// Jackson Coxson

#include <idevice++/lockdown.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/usbmuxd.hpp>
#include <iostream>
#include <plist/plist++.h>

int main() {
    idevice_init_logger(Debug, Disabled, NULL);

    auto u_res = IdeviceFFI::UsbmuxdConnection::default_new(0);
    if_let_err(u_res, e, {
        std::cerr << "failed to connect to usbmuxd";
        std::cerr << e.message;
        return 1;
    });
    auto& u           = u_res.unwrap();

    auto  devices_res = u.get_devices();
    if_let_err(devices_res, e, {
        std::cerr << "failed to get devices from usbmuxd";
        std::cerr << e.message;
        return 1;
    });
    auto devices = std::move(devices_res).unwrap();

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

    IdeviceFFI::UsbmuxdAddr addr     = IdeviceFFI::UsbmuxdAddr::default_new();
    auto                    prov_res = IdeviceFFI::Provider::usbmuxd_new(
        std::move(addr), /*tag*/ 0, udid.unwrap(), id.unwrap(), "reeeeeeeee");
    if_let_err(prov_res, e, {
        std::cerr << "provider failed: " << e.message << "\n";
        return 1;
    });
    auto& prov       = prov_res.unwrap();

    auto  client_res = IdeviceFFI::Lockdown::connect(prov);
    if_let_err(client_res, e, {
        std::cerr << "lockdown connect failed: " << e.message << "\n";
        return 1;
    });
    auto& client = client_res.unwrap();

    auto  pf     = prov.get_pairing_file();
    if_let_err(pf, e, {
        std::cerr << "failed to get pairing file: " << e.message << "\n";
        return 1;
    });
    client.start_session(pf.unwrap());

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
