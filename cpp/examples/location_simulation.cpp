// Jackson Coxson

#include <cstdlib>
#include <iostream>
#include <string>
#include <thread>

#include <idevice++/core_device_proxy.hpp>
#include <idevice++/dvt/location_simulation.hpp>
#include <idevice++/dvt/remote_server.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/readwrite.hpp>
#include <idevice++/rsd.hpp>
#include <idevice++/usbmuxd.hpp>

using namespace IdeviceFFI;

[[noreturn]]
static void die(const char* msg, const FfiError& e) {
    std::cerr << msg << ": " << e.message << "\n";
    std::exit(1);
}

int main(int argc, char** argv) {
    // Usage:
    //   simulate_location clear
    //   simulate_location set <lat> <lon>
    bool           do_clear = false;
    Option<double> lat, lon;

    if (argc == 2 && std::string(argv[1]) == "clear") {
        do_clear = true;
    } else if (argc == 4 && std::string(argv[1]) == "set") {
        lat = std::stod(argv[2]);
        lon = std::stod(argv[3]);
    } else {
        std::cerr << "Usage:\n"
                  << "  " << argv[0] << " clear\n"
                  << "  " << argv[0] << " set <latitude> <longitude>\n";
        return 2;
    }

    // 1) Connect to usbmuxd and pick first device
    auto mux     = UsbmuxdConnection::default_new(/*tag*/ 0).expect("failed to connect to usbmuxd");

    auto devices = mux.get_devices().expect("failed to list devices");
    if (devices.empty()) {
        std::cerr << "no devices connected\n";
        return 1;
    }
    auto& dev  = (devices)[0];

    auto  udid = dev.get_udid();
    if (udid.is_none()) {
        std::cerr << "device has no UDID\n";
        return 1;
    }
    auto mux_id = dev.get_id();
    if (mux_id.is_none()) {
        std::cerr << "device has no mux id\n";
        return 1;
    }

    // 2) Provider via default usbmuxd addr
    auto              addr  = UsbmuxdAddr::default_new();

    const uint32_t    tag   = 0;
    const std::string label = "app_service-jkcoxson";

    auto              provider =
        Provider::usbmuxd_new(std::move(addr), tag, udid.unwrap(), mux_id.unwrap(), label)
            .expect("failed to create provider");

    // 3) CoreDeviceProxy
    auto cdp = CoreDeviceProxy::connect(provider).unwrap_or_else(
        [](FfiError e) -> CoreDeviceProxy { die("failed to connect CoreDeviceProxy", e); });

    auto rsd_port = cdp.get_server_rsd_port().unwrap_or_else(
        [](FfiError err) -> uint16_t { die("failed to get server RSD port", err); });

    // 4) Create software tunnel adapter (consumes proxy)
    auto adapter =
        std::move(cdp).create_tcp_adapter().expect("failed to create software tunnel adapter");

    // 5) Connect adapter to RSD → ReadWrite stream
    auto stream = adapter.connect(rsd_port).expect("failed to connect RSD stream");

    // 6) RSD handshake (consumes stream)
    auto rsd    = RsdHandshake::from_socket(std::move(stream)).expect("failed RSD handshake");

    // 8) RemoteServer over RSD (borrows adapter + handshake)
    auto rs  = RemoteServer::connect_rsd(adapter, rsd).expect("failed to connect to RemoteServer");

    // 9) LocationSimulation client (borrows RemoteServer)
    auto sim = LocationSimulation::create(rs).expect("failed to create LocationSimulation client");

    if (do_clear) {
        sim.clear().expect("clear failed");
        std::cout << "Location cleared!\n";
        return 0;
    }

    // set path
    sim.set(lat.unwrap(), lon.unwrap()).expect("set failed");
    std::cout << "Location set to (" << lat.unwrap() << ", " << lon.unwrap() << ")\n";
    std::cout << "Press Ctrl-C to stop\n";

    // keep process alive like the Rust example
    for (;;) {
        sim.set(lat.unwrap(), lon.unwrap()).expect("set failed");
        std::this_thread::sleep_for(std::chrono::seconds(3));
    }
}
