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

    // 1) usbmuxd → pick first device
    auto     mux = IdeviceFFI::UsbmuxdConnection::default_new(/*tag*/ 0);
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

    // 6) DebugProxy over RSD
    auto diag = DiagnosticsService::connect_rsd(adapter.unwrap(), rsd.unwrap());
    if_let_err(diag, err, { die("failed to connect DebugProxy", err); });

    std::cout << "Getting sysdiagnose, this takes a while! iOS is slow...\n";

    auto cap = diag.unwrap().capture_sysdiagnose(/*dry_run=*/false);
    if_let_err(cap, err, { die("capture_sysdiagnose failed", err); });

    std::cout << "Got sysdiagnose! Saving to file: " << cap.unwrap().preferred_filename << "\n";

    // 7) Stream to file with progress
    std::ofstream out(cap.unwrap().preferred_filename, std::ios::binary);
    if (!out) {
        std::cerr << "failed to open output file\n";
        return 1;
    }

    std::size_t       written = 0;
    const std::size_t total   = cap.unwrap().expected_length;

    for (;;) {
        auto chunk = cap.unwrap().stream.next_chunk();
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
    std::cout << "\nDone! Saved to " << cap.unwrap().preferred_filename << "\n";
    return 0;
}
