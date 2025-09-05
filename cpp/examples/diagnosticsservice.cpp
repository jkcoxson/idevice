// Jackson Coxson

#include <cstdint>
#include <fstream>
#include <iostream>
#include <string>

#include <idevice++/bindings.hpp>
#include <idevice++/core_device_proxy.hpp>
#include <idevice++/diagnosticsservice.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/rsd.hpp>
#include <idevice++/usbmuxd.hpp>

using namespace IdeviceFFI;

[[noreturn]]
static void die(const char* msg, const FfiError& e) {
    std::cerr << msg;
    if (e) {
        std::cerr << ": " << e.message;
    }
    std::cerr << "\n";
    std::exit(1);
}

int main() {
    idevice_init_logger(Debug, Disabled, NULL);
    FfiError err;

    // 1) Connect to usbmuxd and pick first device
    auto     mux = UsbmuxdConnection::default_new(/*tag*/ 0).expect("failed to connect to usbmuxd");

    auto     devices = mux.get_devices().expect("failed to list devices");
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

    // 7) DebugProxy over RSD
    auto diag =
        DiagnosticsService::connect_rsd(adapter, rsd).expect("failed to connect DebugProxy");

    std::cout << "Getting sysdiagnose, this takes a while! iOS is slow...\n";

    auto cap = diag.capture_sysdiagnose(/*dry_run=*/false).expect("capture_sysdiagnose failed");

    std::cout << "Got sysdiagnose! Saving to file: " << cap.preferred_filename << "\n";

    // 7) Stream to file with progress
    std::ofstream out(cap.preferred_filename, std::ios::binary);
    if (!out) {
        std::cerr << "failed to open output file\n";
        return 1;
    }

    std::size_t       written = 0;
    const std::size_t total   = cap.expected_length;

    for (;;) {
        auto chunk = cap.stream.next_chunk();
        match_result(
            chunk,
            res,
            {
                if_let_some(res, chunk_res, {
                    out.write(reinterpret_cast<const char*>(chunk_res.data()),
                              static_cast<std::streamsize>(chunk_res.size()));
                    if (!out) {
                        std::cerr << "write failed\n";
                        return 1;
                    }
                    written += chunk_res.size();
                });
                if (res.is_none()) {
                    break;
                }
            },
            err,
            { die("stream error", err); });
        std::cout << "wrote " << written << "/" << total << " bytes\r" << std::flush;
    }

    out.flush();
    std::cout << "\nDone! Saved to " << cap.preferred_filename << "\n";
    return 0;
}
