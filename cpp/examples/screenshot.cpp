// Jackson Coxson

#include <cstdlib>
#include <fstream>
#include <iostream>
#include <string>

#include <idevice++/core_device_proxy.hpp>
#include <idevice++/dvt/remote_server.hpp>
#include <idevice++/dvt/screenshot.hpp>
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
    //   take_screenshot <output.png>
    if (argc != 2) {
        std::cerr << "Usage:\n"
                  << "  " << argv[0] << " <output.png>\n";
        return 2;
    }

    std::string out_path = argv[1];

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
    const std::string label = "screenshot-client";

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

    // 7) RemoteServer over RSD (borrows adapter + handshake)
    auto rs = RemoteServer::connect_rsd(adapter, rsd).expect("failed to connect to RemoteServer");

    // 8) ScreenshotClient (borrows RemoteServer)
    auto ss = ScreenshotClient::create(rs).unwrap_or_else(
        [](FfiError e) -> ScreenshotClient { die("failed to create ScreenshotClient", e); });

    // 9) Capture screenshot
    auto buf = ss.take_screenshot().unwrap_or_else(
        [](FfiError e) -> std::vector<uint8_t> { die("failed to capture screenshot", e); });

    // 10) Write PNG file
    std::ofstream out(out_path, std::ios::binary);
    if (!out.is_open()) {
        std::cerr << "failed to open output file: " << out_path << "\n";
        return 1;
    }

    out.write(reinterpret_cast<const char*>(buf.data()), static_cast<std::streamsize>(buf.size()));
    out.close();

    std::cout << "Screenshot saved to " << out_path << " (" << buf.size() << " bytes)\n";
    return 0;
}
