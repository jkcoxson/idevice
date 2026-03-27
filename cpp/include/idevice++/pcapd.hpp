// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <memory>
#include <string>
#include <vector>

namespace IdeviceFFI {

using PcapdPtr =
    std::unique_ptr<PcapdClientHandle, FnDeleter<PcapdClientHandle, pcapd_client_free>>;

struct DevicePacket {
    uint32_t             header_length;
    uint8_t              header_version;
    uint32_t             packet_length;
    uint8_t              interface_type;
    uint16_t             unit;
    uint8_t              io;
    uint32_t             protocol_family;
    uint32_t             frame_pre_length;
    uint32_t             frame_post_length;
    std::string          interface_name;
    uint32_t             pid;
    std::string          comm;
    uint32_t             svc;
    uint32_t             epid;
    std::string          ecomm;
    uint32_t             seconds;
    uint32_t             microseconds;
    std::vector<uint8_t> data;
};

class Pcapd {
  public:
    // Factory: connect via Provider
    static Result<Pcapd, FfiError> connect(Provider& provider);

    // Factory: connect via RSD tunnel
    static Result<Pcapd, FfiError> connect_rsd(AdapterHandle* adapter, RsdHandshakeHandle* handshake);

    // Factory: wrap an existing Idevice socket (consumes it on success)
    static Result<Pcapd, FfiError> from_socket(Idevice&& socket);

    // Ops
    Result<DevicePacket, FfiError> next_packet();

    // RAII / moves
    ~Pcapd() noexcept                          = default;
    Pcapd(Pcapd&&) noexcept                    = default;
    Pcapd& operator=(Pcapd&&) noexcept         = default;
    Pcapd(const Pcapd&)                        = delete;
    Pcapd&             operator=(const Pcapd&) = delete;

    PcapdClientHandle* raw() const noexcept { return handle_.get(); }
    static Pcapd       adopt(PcapdClientHandle* h) noexcept { return Pcapd(h); }

  private:
    explicit Pcapd(PcapdClientHandle* h) noexcept : handle_(h) {}
    PcapdPtr handle_{};
};

} // namespace IdeviceFFI
