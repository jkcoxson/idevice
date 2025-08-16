// Jackson Coxson

#include <cstdint>
#include <fstream>
#include <iostream>
#include <optional>
#include <string>

#include <idevice++/bindings.hpp>
#include <idevice++/core_device_proxy.hpp>
#include <idevice++/diagnosticsservice.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/rsd.hpp>
#include <idevice++/usbmuxd.hpp>

using namespace IdeviceFFI;

static void fail(const char* msg, const FfiError& e) {
    std::cerr << msg;
    if (e)
        std::cerr << ": " << e.message;
    std::cerr << "\n";
    std::exit(1);
}

int main() {
    idevice_init_logger(Debug, Disabled, NULL);
    FfiError err;

    // 1) usbmuxd, pick first device
    auto     mux = UsbmuxdConnection::default_new(/*tag*/ 0, err);
    if (!mux)
        fail("failed to connect to usbmuxd", err);

    auto devices = mux->get_devices(err);
    if (!devices)
        fail("failed to list devices", err);
    if (devices->empty()) {
        std::cerr << "no devices connected\n";
        return 1;
    }

    auto& dev    = (*devices)[0];
    auto  udid   = dev.get_udid();
    auto  mux_id = dev.get_id();
    if (!udid || !mux_id) {
        std::cerr << "device missing udid or mux id\n";
        return 1;
    }

    // 2) Provider via default usbmuxd addr
    auto              addr  = UsbmuxdAddr::default_new();

    const uint32_t    tag   = 0;
    const std::string label = "diagnosticsservice-jkcoxson";
    auto provider = Provider::usbmuxd_new(std::move(addr), tag, *udid, *mux_id, label, err);
    if (!provider)
        fail("failed to create provider", err);

    // 3) CoreDeviceProxy
    auto cdp = CoreDeviceProxy::connect(*provider, err);
    if (!cdp)
        fail("failed CoreDeviceProxy connect", err);

    auto rsd_port = cdp->get_server_rsd_port(err);
    if (!rsd_port)
        fail("failed to get RSD port", err);

    // 4) Software tunnel → connect to RSD
    auto adapter = std::move(*cdp).create_tcp_adapter(err);
    if (!adapter)
        fail("failed to create software tunnel adapter", err);

    auto stream = adapter->connect(*rsd_port, err);
    if (!stream)
        fail("failed to connect RSD stream", err);

    // 5) RSD handshake
    auto rsd = RsdHandshake::from_socket(std::move(*stream), err);
    if (!rsd)
        fail("failed RSD handshake", err);
    // 6) Diagnostics Service over RSD
    auto diag = DiagnosticsService::connect_rsd(*adapter, *rsd, err);
    if (!diag)
        fail("failed to connect DiagnosticsService", err);

    std::cout << "Getting sysdiagnose, this takes a while! iOS is slow...\n";

    auto cap = diag->capture_sysdiagnose(/*dry_run=*/false, err);
    if (!cap)
        fail("capture_sysdiagnose failed", err);

    std::cout << "Got sysdiagnose! Saving to file: " << cap->preferred_filename << "\n";

    // 7) Stream to file with progress
    std::ofstream out(cap->preferred_filename, std::ios::binary);
    if (!out) {
        std::cerr << "failed to open output file\n";
        return 1;
    }

    std::size_t       written = 0;
    const std::size_t total   = cap->expected_length;

    for (;;) {
        auto chunk = cap->stream.next_chunk(err);
        if (!chunk) {
            if (err)
                fail("stream error", err); // err set only on real error
            break;                         // nullptr means end-of-stream
        }
        if (!chunk->empty()) {
            out.write(reinterpret_cast<const char*>(chunk->data()),
                      static_cast<std::streamsize>(chunk->size()));
            if (!out) {
                std::cerr << "write failed\n";
                return 1;
            }
            written += chunk->size();
        }
        std::cout << "wrote " << written << "/" << total << " bytes\r" << std::flush;
    }

    out.flush();
    std::cout << "\nDone! Saved to " << cap->preferred_filename << "\n";
    return 0;
}
