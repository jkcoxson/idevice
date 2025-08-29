// Jackson Coxson

#include <iostream>
#include <sstream>
#include <string>
#include <vector>

#include <idevice++/core_device_proxy.hpp>
#include <idevice++/debug_proxy.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/option.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/rsd.hpp>
#include <idevice++/usbmuxd.hpp>

using namespace IdeviceFFI;

[[noreturn]]
static void die(const char* msg, const IdeviceFFI::FfiError& e) {
    std::cerr << msg << ": " << e.message << "\n";
    std::exit(1);
}

static std::vector<std::string> split_args(const std::string& line) {
    std::istringstream       iss(line);
    std::vector<std::string> toks;
    std::string              tok;
    while (iss >> tok) {
        toks.push_back(tok);
    }
    return toks;
}

int main() {
    IdeviceFFI::FfiError err;

    // 1) usbmuxd → pick first device
    auto                 mux = IdeviceFFI::UsbmuxdConnection::default_new(/*tag*/ 0);
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
    auto dbg_res = IdeviceFFI::DebugProxy::connect_rsd(adapter.unwrap(), rsd.unwrap());
    if_let_err(dbg_res, err, { die("failed to connect DebugProxy", err); });
    auto& dbg = dbg_res.unwrap();

    std::cout << "Shell connected! Type 'exit' to quit.\n";
    for (;;) {
        std::cout << "> " << std::flush;

        std::string line;
        if (!std::getline(std::cin, line)) {
            break;
        }
        // trim
        auto first = line.find_first_not_of(" \t\r\n");
        if (first == std::string::npos) {
            continue;
        }
        auto last = line.find_last_not_of(" \t\r\n");
        line      = line.substr(first, last - first + 1);

        if (line == "exit") {
            break;
        }

        // Interpret: first token = command name, rest = argv
        auto toks = split_args(line);
        if (toks.empty()) {
            continue;
        }

        std::string              name = toks.front();
        std::vector<std::string> argv(toks.begin() + 1, toks.end());

        auto                     res = dbg.send_command(name, argv);
        match_result(
            res,
            ok_value,
            { if_let_some(ok_value, some_value, { std::cout << some_value << "\n"; }); },
            err_value,
            { std::cerr << "send_command failed: " << err_value.message << "\n"; });
    }

    return 0;
}
