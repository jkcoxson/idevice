// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/dvt/remote_server.hpp>
#include <idevice++/result.hpp>
#include <memory>
#include <string>

namespace IdeviceFFI {

using NetworkMonitorPtr =
    std::unique_ptr<NetworkMonitorHandle,
                    FnDeleter<NetworkMonitorHandle, network_monitor_free>>;

/// Parsed socket address (IPv4 or IPv6)
struct CppSocketAddress {
    uint8_t     family = 0;
    uint16_t    port   = 0;
    std::string addr;
};

/// A decoded network event from the device
struct NetworkEvent {
    IdeviceNetworkEventType event_type = Unknown;

    // InterfaceDetection
    uint32_t    interface_index = 0;
    std::string interface_name;

    // ConnectionDetection
    CppSocketAddress local_addr;
    CppSocketAddress remote_addr;
    uint32_t         pid               = 0;
    uint64_t         recv_buffer_size  = 0;
    uint64_t         recv_buffer_used  = 0;
    uint64_t         serial_number     = 0;
    uint32_t         kind              = 0;

    // ConnectionUpdate
    uint64_t rx_packets       = 0;
    uint64_t rx_bytes         = 0;
    uint64_t tx_packets       = 0;
    uint64_t tx_bytes         = 0;
    uint64_t rx_dups          = 0;
    uint64_t rx_ooo           = 0;
    uint64_t tx_retx          = 0;
    uint64_t min_rtt          = 0;
    uint64_t avg_rtt          = 0;
    uint64_t connection_serial = 0;
    uint64_t time             = 0;

    // Unknown
    uint64_t unknown_type = 0;
};

class NetworkMonitor {
  public:
    static Result<NetworkMonitor, FfiError> create(RemoteServer& server);

    Result<void, FfiError>          start();
    Result<void, FfiError>          stop();
    /// Blocks until the next event arrives from the device.
    Result<NetworkEvent, FfiError>  next_event();

    ~NetworkMonitor() noexcept                             = default;
    NetworkMonitor(NetworkMonitor&&) noexcept              = default;
    NetworkMonitor& operator=(NetworkMonitor&&) noexcept   = default;
    NetworkMonitor(const NetworkMonitor&)                  = delete;
    NetworkMonitor&       operator=(const NetworkMonitor&) = delete;

    NetworkMonitorHandle* raw() const noexcept { return handle_.get(); }
    static NetworkMonitor adopt(NetworkMonitorHandle* h) noexcept {
        return NetworkMonitor(h);
    }

  private:
    explicit NetworkMonitor(NetworkMonitorHandle* h) noexcept : handle_(h) {}
    NetworkMonitorPtr handle_{};
};

} // namespace IdeviceFFI
