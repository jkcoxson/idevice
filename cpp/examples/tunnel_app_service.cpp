// Jackson Coxson
// Example: connect to a device on the LAN via RPPairing tunnel and list apps

#include <arpa/inet.h>
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <netinet/in.h>
#include <string>
#include <sys/socket.h>

#include <idevice++/app_service.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/rp_pairing_file.hpp>
#include <idevice++/rsd.hpp>
#include <idevice++/tunnel_provider.hpp>

using namespace IdeviceFFI;

static constexpr uint16_t DEFAULT_RPPAIRING_PORT = 49152;

[[noreturn]]
static void die(const char* msg, const FfiError& e) {
    std::cerr << msg << ": " << e.message << " (" << e.code << ")\n";
    std::exit(1);
}

int main(int argc, char** argv) {
    if (argc < 3) {
        std::cerr << "Usage: " << argv[0]
                  << " <ip_address> <pairing_file.plist> [port] [hostname]\n"
                  << "  port defaults to " << DEFAULT_RPPAIRING_PORT << " (remotepairing)\n";
        return 2;
    }

    std::string ip_str       = argv[1];
    std::string pairing_path = argv[2];
    uint16_t    port =
        (argc >= 4) ? static_cast<uint16_t>(std::stoul(argv[3])) : DEFAULT_RPPAIRING_PORT;
    std::string         hostname = (argc >= 5) ? argv[4] : "idevice-cpp-test";

    // 1) Parse IP address into sockaddr with the RSD port
    struct sockaddr_in6 addr6{};
    struct sockaddr_in  addr4{};
    struct sockaddr*    sa;
    socklen_t           sa_len;

    if (inet_pton(AF_INET6, ip_str.c_str(), &addr6.sin6_addr) == 1) {
        addr6.sin6_family = AF_INET6;
        addr6.sin6_port   = htons(port);
        sa                = reinterpret_cast<struct sockaddr*>(&addr6);
        sa_len            = sizeof(addr6);
    } else if (inet_pton(AF_INET, ip_str.c_str(), &addr4.sin_addr) == 1) {
        addr4.sin_family = AF_INET;
        addr4.sin_port   = htons(port);
        sa               = reinterpret_cast<struct sockaddr*>(&addr4);
        sa_len           = sizeof(addr4);
    } else {
        std::cerr << "Invalid IP address: " << ip_str << "\n";
        return 1;
    }

    // 2) Load the RPPairing file
    std::cout << "Loading pairing file: " << pairing_path << "\n";
    auto rpf =
        RpPairingFile::from_file(pairing_path).unwrap_or_else([](FfiError e) -> RpPairingFile {
            die("failed to load pairing file", e);
        });

    // 3) Create tunnel via RPPairing (direct JSON protocol on LAN)
    std::cout << "Creating RPPairing tunnel to " << ip_str << ":" << port << "...\n";
    auto  tunnel_result = create_rppairing_tunnel(sa, sa_len, hostname, rpf)
                              .unwrap_or_else([](FfiError e) -> UsbTunnelResult {
                                 die("failed to create tunnel", e);
                              });

    auto& adapter       = tunnel_result.adapter;
    auto& handshake     = tunnel_result.handshake;

    // 4) Print tunnel info
    auto  uuid          = handshake.uuid().unwrap_or_else(
        [](FfiError e) -> std::string { die("failed to get UUID", e); });
    std::cout << "Tunnel established! Device UUID: " << uuid << "\n";

    auto services = handshake.services().unwrap_or_else(
        [](FfiError e) -> std::vector<RsdService> { die("failed to get services", e); });
    std::cout << "RSD services: " << services.size() << "\n";

    // 5) Connect to AppService over RSD
    std::cout << "Connecting to AppService...\n";
    auto app =
        AppService::connect_rsd(adapter, handshake).unwrap_or_else([](FfiError e) -> AppService {
            die("failed to connect AppService", e);
        });

    // 6) List all apps
    std::cout << "\nInstalled apps:\n";
    auto apps =
        app.list_apps(true, true, true, true, true)
            .unwrap_or_else([](FfiError e) -> std::vector<AppInfo> { die("list_apps failed", e); });

    for (const auto& a : apps) {
        std::string version = a.version.is_some() ? a.version.unwrap() : "?";
        std::cout << "  " << a.name << " (" << a.bundle_identifier << ") v" << version << "\n";
    }
    std::cout << "\nTotal: " << apps.size() << " apps\n";

    return 0;
}
