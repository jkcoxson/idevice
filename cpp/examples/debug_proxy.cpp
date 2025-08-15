// Jackson Coxson

#include <iostream>
#include <sstream>
#include <string>
#include <vector>

#include <idevice++/core_device_proxy.hpp>
#include <idevice++/debug_proxy.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/rsd.hpp>
#include <idevice++/usbmuxd.hpp>

static void die(const char* msg, const IdeviceFFI::FfiError& e) {
    std::cerr << msg << ": " << e.message << "\n";
    std::exit(1);
}

static std::vector<std::string> split_args(const std::string& line) {
    std::istringstream       iss(line);
    std::vector<std::string> toks;
    std::string              tok;
    while (iss >> tok)
        toks.push_back(tok);
    return toks;
}

int main() {
    IdeviceFFI::FfiError err;

    // 1) usbmuxd → pick first device
    auto                 mux = IdeviceFFI::UsbmuxdConnection::default_new(/*tag*/ 0, err);
    if (!mux)
        die("failed to connect to usbmuxd", err);

    auto devices = mux->get_devices(err);
    if (!devices)
        die("failed to list devices", err);
    if (devices->empty()) {
        std::cerr << "no devices connected\n";
        return 1;
    }

    auto& dev  = (*devices)[0];
    auto  udid = dev.get_udid();
    if (!udid) {
        std::cerr << "device has no UDID\n";
        return 1;
    }
    auto mux_id = dev.get_id();
    if (!mux_id) {
        std::cerr << "device has no mux id\n";
        return 1;
    }

    // 2) Provider via default usbmuxd addr
    auto              addr  = IdeviceFFI::UsbmuxdAddr::default_new();

    const uint32_t    tag   = 0;
    const std::string label = "debug-proxy-jkcoxson";
    auto              provider =
        IdeviceFFI::Provider::usbmuxd_new(std::move(addr), tag, *udid, *mux_id, label, err);
    if (!provider)
        die("failed to create provider", err);

    // 3) CoreDeviceProxy
    auto cdp = IdeviceFFI::CoreDeviceProxy::connect(*provider, err);
    if (!cdp)
        die("failed CoreDeviceProxy connect", err);

    auto rsd_port = cdp->get_server_rsd_port(err);
    if (!rsd_port)
        die("failed to get RSD port", err);

    // 4) Software tunnel → stream
    auto adapter = std::move(*cdp).create_tcp_adapter(err);
    if (!adapter)
        die("failed to create software tunnel adapter", err);

    auto stream = adapter->connect(*rsd_port, err);
    if (!stream)
        die("failed to connect RSD stream", err);

    // 5) RSD handshake
    auto rsd = IdeviceFFI::RsdHandshake::from_socket(std::move(*stream), err);
    if (!rsd)
        die("failed RSD handshake", err);

    // 6) DebugProxy over RSD
    auto dbg = IdeviceFFI::DebugProxy::connect_rsd(*adapter, *rsd, err);
    if (!dbg)
        die("failed to connect DebugProxy", err);

    std::cout << "Shell connected! Type 'exit' to quit.\n";
    for (;;) {
        std::cout << "> " << std::flush;

        std::string line;
        if (!std::getline(std::cin, line))
            break;
        // trim
        auto first = line.find_first_not_of(" \t\r\n");
        if (first == std::string::npos)
            continue;
        auto last = line.find_last_not_of(" \t\r\n");
        line      = line.substr(first, last - first + 1);

        if (line == "exit")
            break;

        // Interpret: first token = command name, rest = argv
        auto toks = split_args(line);
        if (toks.empty())
            continue;

        std::string              name = toks.front();
        std::vector<std::string> argv(toks.begin() + 1, toks.end());

        auto                     res = dbg->send_command(name, argv, err);
        if (!res && err) {
            std::cerr << "send_command failed: " << err.message << "\n";
            // clear error for next loop
            err = IdeviceFFI::FfiError{};
            continue;
        }
        if (res && !res->empty()) {
            std::cout << *res << "\n";
        }
    }

    return 0;
}
