// Jackson Coxson

#include <iostream>
#include <string>

#include <idevice++/ffi.hpp>
#include <idevice++/location_simulation.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/usbmuxd.hpp>

using namespace IdeviceFFI;

int main(int argc, char** argv) {
    idevice_init_logger(Debug, Disabled, NULL);

    // Usage:
    //   lockdown_location_simulation clear
    //   lockdown_location_simulation set <latitude> <longitude>
    bool        do_clear = false;
    std::string latitude;
    std::string longitude;

    if (argc == 2 && std::string(argv[1]) == "clear") {
        do_clear = true;
    } else if (argc == 4 && std::string(argv[1]) == "set") {
        latitude = argv[2];
        longitude = argv[3];
    } else {
        std::cerr << "Usage:\n"
                  << "  " << argv[0] << " clear\n"
                  << "  " << argv[0] << " set <latitude> <longitude>\n";
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
    const std::string label = "lockdown-location-simulation";

    auto              provider =
        Provider::usbmuxd_new(std::move(addr), tag, udid.unwrap(), mux_id.unwrap(), label)
            .expect("failed to create provider");

    auto sim = LockdownLocationSimulation::connect(provider)
                   .expect("failed to connect to lockdown location simulation");

    if (do_clear) {
        sim.clear().expect("clear failed");
        std::cout << "Location cleared!\n";
        return 0;
    }

    sim.set(latitude, longitude).expect("set failed");
    std::cout << "Location set to (" << latitude << ", " << longitude << ")\n";
    return 0;
}
