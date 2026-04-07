// Jackson Coxson

#include <cstdint>
#include <cstdlib>
#include <iostream>
#include <string>

#include <idevice++/ffi.hpp>
#include <idevice++/notification_proxy.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/usbmuxd.hpp>

using namespace IdeviceFFI;

int main(int argc, char** argv) {
    idevice_init_logger(Debug, Disabled, NULL);

    if (argc < 3) {
        std::cerr << "Usage:\n"
                  << "  " << argv[0] << " post <notification>\n"
                  << "  " << argv[0] << " observe <notification> [timeout_ms]\n";
        return 2;
    }

    const std::string command = argv[1];
    const std::string name    = argv[2];

    auto mux = UsbmuxdConnection::default_new(0).expect("failed to connect to usbmuxd");

    auto devices = mux.get_devices().expect("failed to list devices");
    if (devices.empty()) {
        std::cerr << "no devices connected\n";
        return 1;
    }

    auto& dev = devices[0];

    auto udid = dev.get_udid();
    if (udid.is_none()) {
        std::cerr << "device has no UDID\n";
        return 1;
    }

    auto mux_id = dev.get_id();
    if (mux_id.is_none()) {
        std::cerr << "device has no mux id\n";
        return 1;
    }

    auto              addr  = UsbmuxdAddr::default_new();
    const uint32_t    tag   = 0;
    const std::string label = "notification-proxy-client";

    auto              provider =
        Provider::usbmuxd_new(std::move(addr), tag, udid.unwrap(), mux_id.unwrap(), label)
            .expect("failed to create provider");

    auto notification_proxy =
        NotificationProxy::connect(provider).expect("failed to connect to notification proxy");

    if (command == "post") {
        if (argc != 3) {
            std::cerr << "Usage:\n"
                      << "  " << argv[0] << " post <notification>\n";
            return 2;
        }

        notification_proxy.post_notification(name).expect("failed to post notification");
        std::cout << "Posted notification: " << name << "\n";
        return 0;
    }

    if (command == "observe") {
        notification_proxy.observe_notification(name).expect("failed to observe notification");
        std::cout << "Waiting for notification: " << name << "\n";

        if (argc == 4) {
            const u_int64_t timeout_ms = std::strtoull(argv[3], nullptr, 10);
            auto received = notification_proxy
                                .receive_notification_with_timeout(timeout_ms)
                                .expect("failed to receive notification");
            std::cout << received << "\n";
            return 0;
        }

        if (argc == 3) {
            auto received = notification_proxy.receive_notification().expect(
                "failed to receive notification");
            std::cout << received << "\n";
            return 0;
        }
    }

    std::cerr << "Usage:\n"
              << "  " << argv[0] << " post <notification>\n"
              << "  " << argv[0] << " observe <notification> [timeout_ms]\n";
    return 2;
}
