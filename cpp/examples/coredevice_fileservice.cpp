// Jackson Coxson
// Browse and download files over the CoreDevice file service.

#include <cstdint>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <string>
#include <vector>

#include <idevice++/core_device_proxy.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/file_service.hpp>
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
              << "  " << argv0 << " ls <domain> <identifier> [path]\n"
              << "  " << argv0 << " cat <domain> <identifier> <path> [out]\n"
              << "  " << argv0 << " touch <domain> <identifier> <path>\n"
              << "\n"
              << "Domains: appDataContainer, appGroupDataContainer, temporary, "
                 "systemCrashLogs\n";
}

int main(int argc, char** argv) {
    if (argc < 4) {
        usage(argv[0]);
        return 2;
    }
    const std::string cmd        = argv[1];
    const std::string domain_arg = argv[2];
    const std::string identifier = argv[3];
    const std::string path       = argc >= 5 ? argv[4] : std::string(".");

    auto              domain     = file_service_domain_from_name(domain_arg);
    if (domain.is_none()) {
        std::cerr << "unknown domain " << domain_arg << "\n";
        usage(argv[0]);
        return 2;
    }

    auto mux     = UsbmuxdConnection::default_new(0).expect("failed to reach usbmuxd");
    auto devices = mux.get_devices().expect("failed to list devices");
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

    auto provider = Provider::usbmuxd_new(UsbmuxdAddr::default_new(),
                                          0,
                                          udid.unwrap(),
                                          mux_id.unwrap(),
                                          "coredevice-fileservice")
                        .expect("failed to create provider");

    auto cdp = CoreDeviceProxy::connect(provider).unwrap_or_else(
        [](FfiError e) -> CoreDeviceProxy { die("failed to connect CoreDeviceProxy", e); });
    auto rsd_port = cdp.get_server_rsd_port().unwrap_or_else(
        [](FfiError e) -> uint16_t { die("failed to get the RSD port", e); });
    auto adapter = std::move(cdp).create_tcp_adapter().expect("failed to create the tunnel");
    auto stream  = adapter.connect(rsd_port).expect("failed to connect the RSD stream");
    auto rsd     = RsdHandshake::from_socket(std::move(stream)).expect("failed the RSD handshake");

    auto files   = FileService::connect_rsd(adapter, rsd).unwrap_or_else([](FfiError e) -> FileService {
        die("failed to connect FileService", e);
    });

    auto session = files.create_session(domain.unwrap(), identifier)
                       .unwrap_or_else([](FfiError e) -> std::string {
                           die("failed to create a session", e);
                       });
    std::cerr << "session " << session << "\n";

    if (cmd == "ls") {
        auto entries = files.list_directory(path).unwrap_or_else(
            [](FfiError e) -> std::vector<std::string> { die("failed to list", e); });
        for (const auto& entry : entries) {
            std::cout << entry << "\n";
        }
        return 0;
    }

    if (cmd == "cat") {
        if (argc < 5) {
            usage(argv[0]);
            return 2;
        }
        // Downloads run on the service's data channel, which the RSD handshake
        // advertises separately from the control channel.
        auto data_service = rsd.service_info(FileService::DATA_SERVICE_NAME);
        if (data_service.is_err()) {
            std::cerr << "the device doesn't advertise " << FileService::DATA_SERVICE_NAME << "\n";
            return 1;
        }

        auto contents = files.retrieve_file(path, adapter, data_service.unwrap().port)
                            .unwrap_or_else([](FfiError e) -> std::vector<uint8_t> {
                                die("failed to retrieve the file", e);
                            });

        if (argc >= 6) {
            std::ofstream out(argv[5], std::ios::binary);
            if (!out) {
                std::cerr << "failed to open " << argv[5] << " for writing\n";
                return 1;
            }
            out.write(reinterpret_cast<const char*>(contents.data()),
                      static_cast<std::streamsize>(contents.size()));
            std::cerr << "wrote " << contents.size() << " bytes to " << argv[5] << "\n";
        } else {
            std::cout.write(reinterpret_cast<const char*>(contents.data()),
                            static_cast<std::streamsize>(contents.size()));
        }
        return 0;
    }

    if (cmd == "touch") {
        if (argc < 5) {
            usage(argv[0]);
            return 2;
        }
        auto res = files.propose_empty_file(path);
        if (res.is_err()) {
            die("failed to propose the file", res.unwrap_err());
        }
        std::cerr << "created " << path << "\n";
        return 0;
    }

    usage(argv[0]);
    return 2;
}
