// Jackson Coxson
// Device configuration over the CoreDevice configuration service: the
// appearance and accessibility toggles Xcode drives.

#include <cstdint>
#include <cstdlib>
#include <iostream>
#include <string>

#include <idevice++/configuration_service.hpp>
#include <idevice++/core_device_proxy.hpp>
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
              << "  " << argv0 << " get\n"
              << "  " << argv0 << " style <light|dark>\n"
              << "  " << argv0 << " opacity <0.0-1.0>\n"
              << "  " << argv0 << " text-size <name>\n"
              << "  " << argv0 << " color-filter <off|preset> [intensity]\n"
              << "  " << argv0 << " reduce-motion <on|off>\n"
              << "  " << argv0 << " reduce-transparency <on|off>\n"
              << "  " << argv0 << " increase-contrast <on|off>\n"
              << "  " << argv0 << " show-borders <on|off>\n";
}

static bool on_off(const std::string& value) { return value == "on" || value == "1"; }

int         main(int argc, char** argv) {
    if (argc < 2) {
        usage(argv[0]);
        return 2;
    }
    const std::string cmd = argv[1];
    const std::string arg = argc >= 3 ? argv[2] : std::string();

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

    auto provider = Provider::usbmuxd_new(UsbmuxdAddr::default_new(),
                                          0,
                                          udid.unwrap(),
                                          mux_id.unwrap(),
                                          "coredevice-configuration")
                        .expect("failed to create provider");

    auto cdp = CoreDeviceProxy::connect(provider).unwrap_or_else(
        [](FfiError e) -> CoreDeviceProxy { die("failed to connect CoreDeviceProxy", e); });
    auto rsd_port = cdp.get_server_rsd_port().unwrap_or_else(
        [](FfiError e) -> uint16_t { die("failed to get the RSD port", e); });
    auto adapter = std::move(cdp).create_tcp_adapter().expect("failed to create the tunnel");
    auto stream  = adapter.connect(rsd_port).expect("failed to connect the RSD stream");
    auto rsd     = RsdHandshake::from_socket(std::move(stream)).expect("failed the RSD handshake");

    auto config  = ConfigurationService::connect_rsd(adapter, rsd)
                      .unwrap_or_else([](FfiError e) -> ConfigurationService {
                          die("failed to connect ConfigurationService", e);
                      });

    if (cmd == "get") {
        auto style = config.get_user_interface_style().unwrap_or_else(
            [](FfiError e) -> UserInterfaceStyle { die("get_user_interface_style failed", e); });
        std::cout << "style: " << (style == UserInterfaceStyle::Dark ? "dark" : "light") << "\n";

        auto size = config.get_device_text_size().unwrap_or_else(
            [](FfiError e) -> std::string { die("get_device_text_size failed", e); });
        std::cout << "text size: " << size << "\n";

        auto filter = config.get_color_filter().unwrap_or_else(
            [](FfiError e) -> ColorFilter { die("get_color_filter failed", e); });
        std::cout << "color filter: " << (filter.enabled ? "on" : "off") << " type="
                  << (filter.filter_type.is_some() ? filter.filter_type.unwrap()
                                                   : std::string("<none>"));
        if (filter.intensity.is_some()) {
            std::cout << " intensity=" << filter.intensity.unwrap();
        }
        std::cout << "\n";

        auto motion = config.get_reduce_motion().unwrap_or_else(
            [](FfiError e) -> bool { die("get_reduce_motion failed", e); });
        std::cout << "reduce motion: " << (motion ? "on" : "off") << "\n";

        auto transparency = config.get_reduce_transparency().unwrap_or_else(
            [](FfiError e) -> bool { die("get_reduce_transparency failed", e); });
        std::cout << "reduce transparency: " << (transparency ? "on" : "off") << "\n";

        auto borders = config.get_show_borders().unwrap_or_else(
            [](FfiError e) -> bool { die("get_show_borders failed", e); });
        std::cout << "show borders: " << (borders ? "on" : "off") << "\n";
        return 0;
    }

    if (argc < 3) {
        usage(argv[0]);
        return 2;
    }

    auto check = [](Result<void, FfiError> r) {
        if (r.is_err()) {
            die("the device rejected the change", r.unwrap_err());
        }
    };

    if (cmd == "style") {
        check(config.set_user_interface_style(arg == "dark" ? UserInterfaceStyle::Dark
                                                            : UserInterfaceStyle::Light));
    } else if (cmd == "opacity") {
        check(config.set_liquid_glass_opacity(std::stof(arg)));
    } else if (cmd == "text-size") {
        check(config.set_device_text_size(arg));
    } else if (cmd == "color-filter") {
        if (arg == "off") {
            check(config.set_color_filter(false, None, None));
        } else {
            Option<float> intensity;
            if (argc >= 4) {
                intensity = std::stof(argv[3]);
            }
            check(config.set_color_filter(true, arg, intensity));
        }
    } else if (cmd == "reduce-motion") {
        check(config.set_reduce_motion(on_off(arg)));
    } else if (cmd == "reduce-transparency") {
        check(config.set_reduce_transparency(on_off(arg)));
    } else if (cmd == "increase-contrast") {
        check(config.set_increase_contrast(on_off(arg)));
    } else if (cmd == "show-borders") {
        check(config.set_show_borders(on_off(arg)));
    } else {
        usage(argv[0]);
        return 2;
    }

    std::cout << "ok\n";
    return 0;
}
