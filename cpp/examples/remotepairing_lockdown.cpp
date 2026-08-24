// Jackson Coxson
// The remotepairingdeviced control channel over USB lockdown.

#include <cstdint>
#include <cstdlib>
#include <iostream>
#include <string>

#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/remote_pairing_lockdown.hpp>
#include <idevice++/rp_pairing_file.hpp>
#include <idevice++/usbmuxd.hpp>

using namespace IdeviceFFI;

[[noreturn]]
static void die(const char* msg, const FfiError& e) {
    std::cerr << msg << ": " << e.message << " (" << e.code << ")\n";
    std::exit(1);
}

static void usage(const char* argv0) {
    std::cerr << "Usage:\n"
              << "  " << argv0 << " info\n"
              << "  " << argv0 << " verify <pairing_file> [hostname]\n"
              << "  " << argv0 << " pair <hostname> <pairing_file>\n";
}

int main(int argc, char** argv) {
    if (argc < 2) {
        usage(argv[0]);
        return 2;
    }
    const std::string cmd      = argv[1];
    const std::string hostname = (cmd == "pair" && argc >= 3) ? argv[2]
                                 : (cmd == "verify" && argc >= 4)
                                     ? argv[3]
                                     : std::string("idevice-cpp-example");

    auto              mux      = UsbmuxdConnection::default_new(0).expect("failed to reach usbmuxd");
    auto              devices  = mux.get_devices().expect("failed to list devices");
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
                                          "remotepairing-lockdown")
                        .expect("failed to create provider");

    auto client = RemotePairingLockdown::connect(provider, hostname)
                      .unwrap_or_else([](FfiError e) -> RemotePairingLockdown {
                          die("failed to reach remotepairingdeviced", e);
                      });

    if (cmd == "info") {
        auto handshake = client.attempt_pair_verify().unwrap_or_else(
            [](FfiError e) -> plist_t { die("the handshake failed", e); });

        char*    xml = nullptr;
        uint32_t len = 0;
        plist_to_xml(handshake, &xml, &len);
        if (xml) {
            std::cout.write(xml, len);
            std::cout << "\n";
            plist_mem_free(xml);
        }
        plist_free(handshake);
        return 0;
    }

    if (cmd == "verify") {
        if (argc < 3) {
            usage(argv[0]);
            return 2;
        }
        auto pairing_file = RpPairingFile::from_file(argv[2])
                                .unwrap_or_else([](FfiError e) -> RpPairingFile {
                                    die("failed to read the pairing file", e);
                                });

        auto handshake = client.attempt_pair_verify().unwrap_or_else(
            [](FfiError e) -> plist_t { die("the handshake failed", e); });
        plist_free(handshake);

        auto res = client.validate_pairing(pairing_file);
        if (res.is_err()) {
            std::cerr << "pair-verify failed: " << res.unwrap_err().message << "\n";
            return 1;
        }
        std::cout << "the device recognizes this pairing record\n";
        return 0;
    }

    if (cmd == "pair") {
        if (argc < 4) {
            usage(argv[0]);
            return 2;
        }
        const std::string output_path  = argv[3];

        auto              pairing_file = RpPairingFile::generate(hostname)
                                .unwrap_or_else([](FfiError e) -> RpPairingFile {
                                    die("failed to generate a pairing file", e);
                                });

        // Over the already-trusted USB transport there is no Trust prompt, so
        // the PIN is never asked for.
        auto res = client.connect_pairing(pairing_file);
        if (res.is_err()) {
            die("pairing failed", res.unwrap_err());
        }

        auto written = pairing_file.write(output_path);
        if (written.is_err()) {
            die("failed to save the pairing file", written.unwrap_err());
        }
        std::cout << "paired, saved to " << output_path << "\n";
        return 0;
    }

    usage(argv[0]);
    return 2;
}
