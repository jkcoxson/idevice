// Jackson Coxson
// Example: advertise this computer as a "pairable host"
// (_remotepairing-pairable-host._tcp) and accept a device-initiated pairing.
//
// Starting with iOS 27 the device initiates pairing to the computer: it browses
// for the advertised service, the user taps to pair, the device connects, and we
// display a PIN the user types into the device. Mirrors tools/src/pair_host.rs.

#include <cstdint>
#include <cstdio>
#include <iostream>
#include <string>

#include <idevice++/ffi.hpp>
#include <idevice++/pairable_host.hpp>
#include <idevice++/rp_pairing_file.hpp>

using namespace IdeviceFFI;

static void on_pin(const char* pin, void* /*context*/) {
    std::cout << "\n========================================\n"
              << "  Enter this code on your device: " << pin << "\n"
              << "========================================\n"
              << std::endl;
}

int main(int argc, char** argv) {
    if (argc >= 2 && (std::string(argv[1]) == "-h" || std::string(argv[1]) == "--help")) {
        std::cerr << "Usage: " << argv[0] << " [name] [model] [port] [out_pairing_file]\n"
                  << "  name   defaults to \"idevice-cpp\"\n"
                  << "  model  defaults to \"Mac17,7\"\n"
                  << "  port   defaults to 0 (pick a free port)\n"
                  << "  out    defaults to \"host_pairing_file.plist\"\n";
        return 2;
    }

    std::string name  = (argc >= 2) ? argv[1] : "idevice-cpp";
    std::string model = (argc >= 3) ? argv[2] : "Mac17,7";
    uint16_t    port  = (argc >= 4) ? static_cast<uint16_t>(std::stoul(argv[3])) : 0;
    std::string out   = (argc >= 5) ? argv[4] : "host_pairing_file.plist";

    idevice_init_logger(Info, Disabled, nullptr);

    std::cout << "Advertising _remotepairing-pairable-host._tcp as \"" << name << "\" (" << model
              << ")\n"
              << "Waiting for a device to connect and start pairing...\n"
              << std::flush;

    auto res = accept_pairing(name, model, port, on_pin, nullptr);
    if (res.is_err()) {
        const auto& e = res.unwrap_err();
        std::cerr << "Pairing failed: " << e.message << " (code " << e.code << ")\n";
        return 1;
    }

    auto result = std::move(res).unwrap();

    auto w = result.pairing_file.write(out);
    if (w.is_err()) {
        const auto& e = w.unwrap_err();
        std::cerr << "Failed to write pairing file: " << e.message << " (code " << e.code << ")\n";
        return 1;
    }

    std::cout << "\nPairing succeeded! Wrote pairing file to " << out << ".\n";
    return 0;
}
