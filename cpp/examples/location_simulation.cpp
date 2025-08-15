// Jackson Coxson

#include <cstdlib>
#include <iostream>
#include <optional>
#include <string>
#include <thread>

#include <idevice++/core_device_proxy.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/location_simulation.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/readwrite.hpp>
#include <idevice++/remote_server.hpp>
#include <idevice++/rsd.hpp>
#include <idevice++/usbmuxd.hpp>

using namespace IdeviceFFI;

static void die(const char* msg, const FfiError& e) {
    std::cerr << msg << ": " << e.message << "\n";
    std::exit(1);
}

int main(int argc, char** argv) {
    // Usage:
    //   simulate_location clear
    //   simulate_location set <lat> <lon>
    bool                  do_clear = false;
    std::optional<double> lat, lon;

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

    FfiError err;

    // 1) Connect to usbmuxd and pick first device
    auto     mux = UsbmuxdConnection::default_new(/*tag*/ 0, err);
    if (!mux)
        die("failed to connect to usbmuxd", err);

    auto devices = mux->get_devices(err);
    if (!devices)
        die("failed to list devices", err);
    if (devices->empty()) {
        std::cerr << "no devices connected\n";
        return 1;
    }
    auto& dev     = (*devices)[0];

    auto  udidOpt = dev.get_udid();
    if (!udidOpt) {
        std::cerr << "device has no UDID\n";
        return 1;
    }
    auto idOpt = dev.get_id();
    if (!idOpt) {
        std::cerr << "device has no mux id\n";
        return 1;
    }

    // 2) Make a Provider for this device via default addr
    auto              addr  = UsbmuxdAddr::default_new();

    const uint32_t    tag   = 0;
    const std::string label = "simulate_location-jkcoxson";

    auto provider = Provider::usbmuxd_new(std::move(addr), tag, *udidOpt, *idOpt, label, err);
    if (!provider)
        die("failed to create provider", err);

    // 3) Connect CoreDeviceProxy (borrow provider)
    auto cdp = CoreDeviceProxy::connect(*provider, err);
    if (!cdp)
        die("failed to connect CoreDeviceProxy", err);

    // 4) Read handshake’s server RSD port
    auto rsd_port = cdp->get_server_rsd_port(err);
    if (!rsd_port)
        die("failed to get server RSD port", err);

    // 5) Create software tunnel adapter (consumes proxy)
    auto adapter = std::move(*cdp).create_tcp_adapter(err);
    if (!adapter)
        die("failed to create software tunnel adapter", err);

    // 6) Connect adapter to RSD port → ReadWrite stream
    auto stream = adapter->connect(*rsd_port, err);
    if (!stream)
        die("failed to connect RSD stream", err);

    // 7) RSD handshake (consumes stream)
    auto rsd = RsdHandshake::from_socket(std::move(*stream), err);
    if (!rsd)
        die("failed RSD handshake", err);

    // 8) RemoteServer over RSD (borrows adapter + handshake)
    auto rs = RemoteServer::connect_rsd(*adapter, *rsd, err);
    if (!rs)
        die("failed to connect RemoteServer", err);

    // 9) LocationSimulation client (borrows RemoteServer)
    auto sim = LocationSimulation::create(*rs, err);
    if (!sim)
        die("failed to create LocationSimulation client", err);

    if (do_clear) {
        if (!sim->clear(err))
            die("clear failed", err);
        std::cout << "Location cleared!\n";
        return 0;
    }

    // set path
    if (!sim->set(*lat, *lon, err))
        die("set failed", err);
    std::cout << "Location set to (" << *lat << ", " << *lon << ")\n";
    std::cout << "Press Ctrl-C to stop\n";

    // keep process alive like the Rust example
    for (;;) {
        if (!sim->set(*lat, *lon, err))
            die("set failed", err);
        std::this_thread::sleep_for(std::chrono::seconds(3));
    }
}
