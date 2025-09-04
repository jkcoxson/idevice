// Jackson Coxson

#include <cstdlib>
#include <fstream>
#include <iostream>
#include <string>
#include <vector>

#include <idevice++/app_service.hpp>
#include <idevice++/core_device_proxy.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/option.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/readwrite.hpp>
#include <idevice++/rsd.hpp>
#include <idevice++/usbmuxd.hpp>

using namespace IdeviceFFI;

[[noreturn]]
static void die(const char* msg, const FfiError& e) {
    std::cerr << msg << ": " << e.message << "(" << e.code << ")\n";
    std::exit(1);
}

static void usage(const char* argv0) {
    std::cerr << "Usage:\n"
              << "  " << argv0 << " list\n"
              << "  " << argv0 << " launch <bundle_id>\n"
              << "  " << argv0 << " processes\n"
              << "  " << argv0 << " uninstall <bundle_id>\n"
              << "  " << argv0 << " signal <pid> <signal>\n"
              << "  " << argv0 << " icon <bundle_id> <save_path> [hw=1.0] [scale=1.0]\n";
}

int main(int argc, char** argv) {
    if (argc < 2) {
        usage(argv[0]);
        return 2;
    }

    std::string cmd = argv[1];

    FfiError    err;

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

    // 7) AppService over RSD (borrows adapter + handshake)
    auto app = AppService::connect_rsd(adapter, rsd).unwrap_or_else([&](FfiError e) -> AppService {
        die("failed to connect AppService", e); // never returns
    });

    // 8) Commands
    if (cmd == "list") {
        auto apps = app.list_apps(/*app_clips*/ true,
                                  /*removable*/ true,
                                  /*hidden*/ true,
                                  /*internal*/ true,
                                  /*default_apps*/ true)
                        .unwrap_or_else(
                            [](FfiError e) -> std::vector<AppInfo> { die("list_apps failed", e); });

        for (const auto& a : apps) {
            std::cout << "- " << a.bundle_identifier << " | name=" << a.name << " | version="
                      << (a.version.is_some() ? a.version.unwrap() : std::string("<none>"))
                      << " | dev=" << (a.is_developer_app ? "y" : "n")
                      << " | hidden=" << (a.is_hidden ? "y" : "n") << "\n";
        }
        return 0;

    } else if (cmd == "launch") {
        if (argc < 3) {
            std::cerr << "No bundle ID passed\n";
            return 2;
        }
        std::string              bundle_id = argv[2];

        std::vector<std::string> args; // empty in this example
        auto                     resp =
            app.launch(bundle_id,
                       args,
                       /*kill_existing*/ false,
                       /*start_suspended*/ false)
                .unwrap_or_else([](FfiError e) -> LaunchResponse { die("launch failed", e); });

        std::cout << "Launched pid=" << resp.pid << " exe=" << resp.executable_url
                  << " piv=" << resp.process_identifier_version
                  << " audit_token_len=" << resp.audit_token.size() << "\n";
        return 0;

    } else if (cmd == "processes") {
        auto procs = app.list_processes().unwrap_or_else(
            [](FfiError e) -> std::vector<ProcessToken> { die("list_processes failed", e); });

        for (const auto& p : procs) {
            std::cout << p.pid << " : "
                      << (p.executable_url.is_some() ? p.executable_url.unwrap()
                                                     : std::string("<none>"))
                      << "\n";
        }
        return 0;

    } else if (cmd == "uninstall") {
        if (argc < 3) {
            std::cerr << "No bundle ID passed\n";
            return 2;
        }
        std::string bundle_id = argv[2];

        app.uninstall(bundle_id).expect("Uninstall failed");
        std::cout << "Uninstalled " << bundle_id << "\n";
        return 0;

    } else if (cmd == "signal") {
        if (argc < 4) {
            std::cerr << "Usage: signal <pid> <signal>\n";
            return 2;
        }
        uint32_t pid    = static_cast<uint32_t>(std::stoul(argv[2]));
        uint32_t signal = static_cast<uint32_t>(std::stoul(argv[3]));

        auto res = app.send_signal(pid, signal).unwrap_or_else([](FfiError e) -> SignalResponse {
            die("send_signal failed", e);
        });

        std::cout << "Signaled pid=" << res.pid << " signal=" << res.signal
                  << " ts_ms=" << res.device_timestamp_ms << " exe="
                  << (res.executable_url.is_some() ? res.executable_url.unwrap()
                                                   : std::string("<none>"))
                  << "\n";
        return 0;

    } else if (cmd == "icon") {
        if (argc < 4) {
            std::cerr << "Usage: icon <bundle_id> <save_path> [hw=1.0] [scale=1.0]\n";
            return 2;
        }
        std::string bundle_id = argv[2];
        std::string save_path = argv[3];
        float       hw        = (argc >= 5) ? std::stof(argv[4]) : 1.0f;
        float       scale     = (argc >= 6) ? std::stof(argv[5]) : 1.0f;

        auto        icon =
            app.fetch_icon(bundle_id, hw, hw, scale, /*allow_placeholder*/ true)
                .unwrap_or_else([](FfiError e) -> IconData { die("fetch_app_icon failed", e); });

        std::ofstream out(save_path, std::ios::binary);
        if (!out) {
            std::cerr << "Failed to open " << save_path << " for writing\n";
            return 1;
        }
        out.write(reinterpret_cast<const char*>(icon.data.data()),
                  static_cast<std::streamsize>(icon.data.size()));
        out.close();

        std::cout << "Saved icon to " << save_path << " (" << icon.data.size() << " bytes, "
                  << icon.icon_width << "x" << icon.icon_height << ", min " << icon.minimum_width
                  << "x" << icon.minimum_height << ")\n";
        return 0;

    } else {
        usage(argv[0]);
        return 2;
    }
}
