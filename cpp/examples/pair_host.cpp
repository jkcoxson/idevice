// Jackson Coxson
// Example: advertise this computer as a "pairable host"
// (_remotepairing-pairable-host._tcp) and accept a device-initiated pairing.
//
// Starting with iOS 27 the device initiates pairing to the computer: it browses
// for the advertised service, the user taps to pair, the device connects, and we
// display a PIN the user types into the device. Mirrors tools/src/pair_host.rs.

#include <cstdint>
#include <iostream>
#include <string>
#include <utility>

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
        std::cerr << "Usage: " << argv[0]
                  << " [name] [model] [port] [out_pairing_file] [--pinless]\n"
                  << "  name     defaults to \"idevice-cpp\"\n"
                  << "  model    defaults to \"Mac17,7\"\n"
                  << "  port     defaults to 0 (pick a free port)\n"
                  << "  out      defaults to \"host_pairing_file.plist\"\n"
                  << "  --pinless advertise pinless pairing and display 000000\n";
        return 2;
    }

    std::string name  = "idevice-cpp";
    std::string model = "Mac17,7";
    uint16_t    port  = 0;
    std::string out   = "host_pairing_file.plist";
    bool        pinless = false;
    int         positional = 0;
    for (int i = 1; i < argc; ++i) {
        std::string arg = argv[i];
        if (arg == "--pinless") {
            pinless = true;
            continue;
        }
        switch (positional++) {
        case 0: name = std::move(arg); break;
        case 1: model = std::move(arg); break;
        case 2: port = static_cast<uint16_t>(std::stoul(arg)); break;
        case 3: out = std::move(arg); break;
        default: break;
        }
    }

    idevice_init_logger(Info, Disabled, nullptr);

    std::cout << "Advertising _remotepairing-pairable-host._tcp as \"" << name << "\" (" << model
              << ")\n"
              << "Waiting for a device to connect and start pairing...\n"
              << std::flush;

    auto res = accept_pairing(name, model, port, on_pin, nullptr, nullptr, pinless);
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
