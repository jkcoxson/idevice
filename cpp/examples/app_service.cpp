// Jackson Coxson

#include <cstdlib>
#include <fstream>
#include <iostream>
#include <string>
#include <vector>

#include <idevice++/app_service.hpp>
#include <idevice++/core_device_proxy.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/readwrite.hpp>
#include <idevice++/rsd.hpp>
#include <idevice++/usbmuxd.hpp>

using namespace IdeviceFFI;

static void die(const char* msg, const FfiError& e) {
    std::cerr << msg << ": " << e.message << "\n";
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
    auto        mux = UsbmuxdConnection::default_new(/*tag*/ 0, err);
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
    auto              addr  = UsbmuxdAddr::default_new();

    const uint32_t    tag   = 0;
    const std::string label = "app_service-jkcoxson";

    auto provider = Provider::usbmuxd_new(std::move(addr), tag, *udid, *mux_id, label, err);
    if (!provider)
        die("failed to create provider", err);

    // 3) CoreDeviceProxy
    auto cdp = CoreDeviceProxy::connect(*provider, err);
    if (!cdp)
        die("failed to connect CoreDeviceProxy", err);

    auto rsd_port = cdp->get_server_rsd_port(err);
    if (!rsd_port)
        die("failed to get server RSD port", err);

    // 4) Create software tunnel adapter (consumes proxy)
    auto adapter = std::move(*cdp).create_tcp_adapter(err);
    if (!adapter)
        die("failed to create software tunnel adapter", err);

    // 5) Connect adapter to RSD → ReadWrite stream
    auto stream = adapter->connect(*rsd_port, err);
    if (!stream)
        die("failed to connect RSD stream", err);

    // 6) RSD handshake (consumes stream)
    auto rsd = RsdHandshake::from_socket(std::move(*stream), err);
    if (!rsd)
        die("failed RSD handshake", err);

    // 7) AppService over RSD (borrows adapter + handshake)
    auto app = AppService::connect_rsd(*adapter, *rsd, err);
    if (!app)
        die("failed to connect AppService", err);

    // 8) Commands
    if (cmd == "list") {
        auto apps = app->list_apps(/*app_clips*/ true,
                                   /*removable*/ true,
                                   /*hidden*/ true,
                                   /*internal*/ true,
                                   /*default_apps*/ true,
                                   err);
        if (!apps)
            die("list_apps failed", err);

        for (const auto& a : *apps) {
            std::cout << "- " << a.bundle_identifier << " | name=" << a.name
                      << " | version=" << (a.version ? *a.version : std::string("<none>"))
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
        auto                     resp = app->launch(bundle_id,
                                args,
                                /*kill_existing*/ false,
                                /*start_suspended*/ false,
                                err);
        if (!resp)
            die("launch failed", err);

        std::cout << "Launched pid=" << resp->pid << " exe=" << resp->executable_url
                  << " piv=" << resp->process_identifier_version
                  << " audit_token_len=" << resp->audit_token.size() << "\n";
        return 0;

    } else if (cmd == "processes") {
        auto procs = app->list_processes(err);
        if (!procs)
            die("list_processes failed", err);

        for (const auto& p : *procs) {
            std::cout << p.pid << " : "
                      << (p.executable_url ? *p.executable_url : std::string("<none>")) << "\n";
        }
        return 0;

    } else if (cmd == "uninstall") {
        if (argc < 3) {
            std::cerr << "No bundle ID passed\n";
            return 2;
        }
        std::string bundle_id = argv[2];

        if (!app->uninstall(bundle_id, err))
            die("uninstall failed", err);
        std::cout << "Uninstalled " << bundle_id << "\n";
        return 0;

    } else if (cmd == "signal") {
        if (argc < 4) {
            std::cerr << "Usage: signal <pid> <signal>\n";
            return 2;
        }
        uint32_t pid    = static_cast<uint32_t>(std::stoul(argv[2]));
        uint32_t signal = static_cast<uint32_t>(std::stoul(argv[3]));

        auto     res    = app->send_signal(pid, signal, err);
        if (!res)
            die("send_signal failed", err);

        std::cout << "Signaled pid=" << res->pid << " signal=" << res->signal
                  << " ts_ms=" << res->device_timestamp_ms
                  << " exe=" << (res->executable_url ? *res->executable_url : std::string("<none>"))
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

        auto icon = app->fetch_icon(bundle_id, hw, hw, scale, /*allow_placeholder*/ true, err);
        if (!icon)
            die("fetch_app_icon failed", err);

        std::ofstream out(save_path, std::ios::binary);
        if (!out) {
            std::cerr << "Failed to open " << save_path << " for writing\n";
            return 1;
        }
        out.write(reinterpret_cast<const char*>(icon->data.data()),
                  static_cast<std::streamsize>(icon->data.size()));
        out.close();

        std::cout << "Saved icon to " << save_path << " (" << icon->data.size() << " bytes, "
                  << icon->icon_width << "x" << icon->icon_height << ", min " << icon->minimum_width
                  << "x" << icon->minimum_height << ")\n";
        return 0;

    } else {
        usage(argv[0]);
        return 2;
    }
}
