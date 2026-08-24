// Jackson Coxson
// Talk to cryptexd, the daemon that installs the DeveloperDiskImage cryptex on
// iOS 17+.

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <iostream>
#include <string>
#include <vector>

#include <idevice++/core_device_proxy.hpp>
#include <idevice++/cryptexd.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/rsd.hpp>
#include <idevice++/usbmuxd.hpp>

using namespace IdeviceFFI;

[[noreturn]]
static void die(const char* msg, const FfiError& e) {
    std::cerr << msg << ": " << e.message << " (" << e.code << ")\n";
    std::exit(1);
}

static void usage(const char* argv0) {
    std::cerr << "Usage:\n"
              << "  " << argv0 << " list\n"
              << "  " << argv0 << " nonce\n"
              << "  " << argv0 << " ddi\n"
              << "  " << argv0 << " install-ddi <restore_dir>\n"
              << "  " << argv0 << " uninstall <identifier> [version]\n";
}

static void print_hex(const std::vector<uint8_t>& data) {
    for (uint8_t byte : data) {
        std::printf("%02x", byte);
    }
    std::printf("\n");
}

int main(int argc, char** argv) {
    if (argc < 2) {
        usage(argv[0]);
        return 2;
    }
    const std::string cmd     = argv[1];

    auto              mux     = UsbmuxdConnection::default_new(0).expect("failed to reach usbmuxd");
    auto              devices = mux.get_devices().expect("failed to list devices");
    if (devices.empty()) {
        std::cerr << "no devices connected\n";
        return 1;
    }
    auto& dev    = devices[0];
    auto  udid   = dev.get_udid();
    auto  mux_id = dev.get_id();
    if (udid.is_none() || mux_id.is_none()) {
        std::cerr << "device has no UDID or mux id\n";
        return 1;
    }

    auto provider =
        Provider::usbmuxd_new(UsbmuxdAddr::default_new(), 0, udid.unwrap(), mux_id.unwrap(), "cryptexd")
            .expect("failed to create provider");

    auto cdp = CoreDeviceProxy::connect(provider).unwrap_or_else(
        [](FfiError e) -> CoreDeviceProxy { die("failed to connect CoreDeviceProxy", e); });
    auto rsd_port = cdp.get_server_rsd_port().unwrap_or_else(
        [](FfiError e) -> uint16_t { die("failed to get the RSD port", e); });
    auto adapter = std::move(cdp).create_tcp_adapter().expect("failed to create the tunnel");
    auto stream  = adapter.connect(rsd_port).expect("failed to connect the RSD stream");
    auto rsd     = RsdHandshake::from_socket(std::move(stream)).expect("failed the RSD handshake");

    // The daemon serves one routine per connection, so each command below
    // connects its own client.
    if (cmd == "list") {
        auto client = Cryptexd::connect_rsd(adapter, rsd).unwrap_or_else([](FfiError e) -> Cryptexd {
            die("failed to connect cryptexd", e);
        });
        auto installed = client.copy_installed().unwrap_or_else(
            [](FfiError e) -> std::vector<InstalledCryptex> { die("copy_installed failed", e); });
        for (const auto& cryptex : installed) {
            std::cout << cryptex.identifier << " " << cryptex.version << "\n";
        }
        return 0;
    }

    if (cmd == "nonce") {
        auto client = Cryptexd::connect_rsd(adapter, rsd).unwrap_or_else([](FfiError e) -> Cryptexd {
            die("failed to connect cryptexd", e);
        });
        auto nonce = client.get_nonce(NonceDomain::cryptex())
                         .unwrap_or_else([](FfiError e) -> std::vector<uint8_t> {
                             die("get_nonce failed", e);
                         });
        print_hex(nonce);
        return 0;
    }

    if (cmd == "ddi") {
        auto installed = cryptexd_installed_ddi(adapter, rsd)
                             .unwrap_or_else([](FfiError e) -> Option<InstalledCryptex> {
                                 die("installed_ddi failed", e);
                             });
        if (installed.is_none()) {
            std::cout << "no DDI installed\n";
            return 1;
        }
        auto ddi = installed.unwrap();
        std::cout << ddi.identifier << " " << ddi.version << "\n";
        return 0;
    }

    if (cmd == "install-ddi") {
        if (argc < 3) {
            usage(argv[0]);
            return 2;
        }
        auto assets = Cryptex1Assets::load(argv[2]).unwrap_or_else([](FfiError e) -> Cryptex1Assets {
            die("failed to load the DDI assets", e);
        });
        auto installed = cryptexd_install_ddi(adapter, rsd, assets)
                             .unwrap_or_else([](FfiError e) -> InstalledCryptex {
                                 die("failed to install the DDI", e);
                             });
        std::cout << "installed " << installed.identifier << " " << installed.version << "\n";
        return 0;
    }

    if (cmd == "uninstall") {
        if (argc < 3) {
            usage(argv[0]);
            return 2;
        }
        auto client = Cryptexd::connect_rsd(adapter, rsd).unwrap_or_else([](FfiError e) -> Cryptexd {
            die("failed to connect cryptexd", e);
        });
        Option<std::string> version;
        if (argc >= 4) {
            version = std::string(argv[3]);
        }
        auto res = client.uninstall(argv[2], version);
        if (res.is_err()) {
            die("uninstall failed", res.unwrap_err());
        }
        std::cout << "uninstalled " << argv[2] << "\n";
        return 0;
    }

    usage(argv[0]);
    return 2;
}
