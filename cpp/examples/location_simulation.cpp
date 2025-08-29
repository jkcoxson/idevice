// Jackson Coxson

#include <cstdlib>
#include <iostream>
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

    // 1) usbmuxd → pick first device
    auto mux = IdeviceFFI::UsbmuxdConnection::default_new(/*tag*/ 0);
    if_let_err(mux, err, { die("failed to connect to usbmuxd", err); });

    auto devices = mux.unwrap().get_devices();
    if_let_err(devices, err, { die("failed to list devices", err); });
    if (devices.unwrap().empty()) {
        std::cerr << "no devices connected\n";
        return 1;
    }

    auto& dev  = (devices.unwrap())[0];
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
    auto              addr     = IdeviceFFI::UsbmuxdAddr::default_new();

    const uint32_t    tag      = 0;
    const std::string label    = "debug-proxy-jkcoxson";
    auto              provider = IdeviceFFI::Provider::usbmuxd_new(
        std::move(addr), tag, udid.unwrap(), mux_id.unwrap(), label);
    if_let_err(provider, err, { die("failed to create provider", err); });

    // 3) CoreDeviceProxy
    auto cdp = CoreDeviceProxy::connect(provider.unwrap())
                   .unwrap_or_else([](FfiError e) -> CoreDeviceProxy {
                       die("failed to connect CoreDeviceProxy", e);
                   });

    auto rsd_port = cdp.get_server_rsd_port().unwrap_or_else(
        [](FfiError err) -> uint16_t { die("failed to get server RSD port", err); });

    // 4) Create software tunnel adapter (consumes proxy)
    auto adapter = std::move(cdp).create_tcp_adapter();
    if_let_err(adapter, err, { die("failed to create software tunnel adapter", err); });

    // 5) Connect adapter to RSD → ReadWrite stream
    auto stream = adapter.unwrap().connect(rsd_port);
    if_let_err(stream, err, { die("failed to connect RSD stream", err); });

    // 6) RSD handshake (consumes stream)
    auto rsd = RsdHandshake::from_socket(std::move(stream.unwrap()));
    if_let_err(rsd, err, { die("failed RSD handshake", err); });

    // 8) RemoteServer over RSD (borrows adapter + handshake)
    auto rs = RemoteServer::connect_rsd(adapter.unwrap(), rsd.unwrap());
    if_let_err(rs, err, { die("failed to connect RemoteServer", err); });

    // 9) LocationSimulation client (borrows RemoteServer)
    auto sim_res = LocationSimulation::create(rs.unwrap());
    if_let_err(sim_res, err, { die("failed to create LocationSimulation client", err); });
    auto& sim = sim_res.unwrap();

    if (do_clear) {
        if_let_err(sim.clear(), err, { die("clear failed", err); });
        std::cout << "Location cleared!\n";
        return 0;
    }

    // set path
    if_let_err(sim.set(lat.unwrap(), lon.unwrap()), err, { die("set failed", err); });
    std::cout << "Location set to (" << lat.unwrap() << ", " << lon.unwrap() << ")\n";
    std::cout << "Press Ctrl-C to stop\n";

    // keep process alive like the Rust example
    for (;;) {
        if_let_err(sim.set(lat.unwrap(), lon.unwrap()), err, { die("set failed", err); });
        std::this_thread::sleep_for(std::chrono::seconds(3));
    }
}
