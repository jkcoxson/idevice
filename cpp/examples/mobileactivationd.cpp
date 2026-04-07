// Jackson Coxson

#include <iostream>
#include <string>

#include <idevice++/ffi.hpp>
#include <idevice++/mobileactivationd.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/usbmuxd.hpp>

using namespace IdeviceFFI;

int main(int argc, char** argv) {
    idevice_init_logger(Debug, Disabled, NULL);

    bool deactivate = false;
    if (argc == 2 && std::string(argv[1]) == "deactivate") {
        deactivate = true;
    } else if (argc > 2) {
        std::cerr << "Usage:\n"
                  << "  " << argv[0] << "\n"
                  << "  " << argv[0] << " deactivate\n";
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
    const std::string label = "mobileactivationd-client";

    auto              provider =
        Provider::usbmuxd_new(std::move(addr), tag, udid.unwrap(), mux_id.unwrap(), label)
            .expect("failed to create provider");

    auto mobileactivationd =
        MobileActivationd::connect(provider).expect("failed to connect to mobileactivationd");

    auto state      = mobileactivationd.get_state().expect("failed to get activation state");
    auto activated  = mobileactivationd.is_activated().expect("failed to get activation status");

    std::cout << "Activation state: " << state << "\n";
    std::cout << "Activated: " << (activated ? "true" : "false") << "\n";

    if (deactivate) {
        mobileactivationd.deactivate().expect("failed to deactivate device");
        std::cout << "Deactivation request sent\n";
    }

    return 0;
}
