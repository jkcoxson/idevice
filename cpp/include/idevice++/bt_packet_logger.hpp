// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <memory>
#include <vector>

namespace IdeviceFFI {

using BtPacketLoggerPtr =
    std::unique_ptr<BtPacketLoggerClientHandle,
                    FnDeleter<BtPacketLoggerClientHandle, bt_packet_logger_client_free>>;

struct BtPacket {
    uint32_t             length;
    uint32_t             ts_secs;
    uint32_t             ts_usecs;
    uint8_t              kind;
    std::vector<uint8_t> h4_data;
};

class BtPacketLogger {
  public:
    // Factory: connect via Provider
    static Result<BtPacketLogger, FfiError> connect(Provider& provider);

    // Factory: wrap an existing Idevice socket (consumes it on success)
    static Result<BtPacketLogger, FfiError> from_socket(Idevice&& socket);

    // Ops
    Result<Option<BtPacket>, FfiError>      next_packet();

    // RAII / moves
    ~BtPacketLogger() noexcept                                   = default;
    BtPacketLogger(BtPacketLogger&&) noexcept                    = default;
    BtPacketLogger& operator=(BtPacketLogger&&) noexcept         = default;
    BtPacketLogger(const BtPacketLogger&)                        = delete;
    BtPacketLogger&             operator=(const BtPacketLogger&) = delete;

    BtPacketLoggerClientHandle* raw() const noexcept { return handle_.get(); }
    static BtPacketLogger       adopt(BtPacketLoggerClientHandle* h) noexcept {
        return BtPacketLogger(h);
    }

  private:
    explicit BtPacketLogger(BtPacketLoggerClientHandle* h) noexcept : handle_(h) {}
    BtPacketLoggerPtr handle_{};
};

} // namespace IdeviceFFI
