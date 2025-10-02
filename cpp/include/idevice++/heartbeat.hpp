#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <memory>
#include <sys/_types/_u_int64_t.h>

namespace IdeviceFFI {

using HeartbeatPtr =
    std::unique_ptr<HeartbeatClientHandle, FnDeleter<HeartbeatClientHandle, heartbeat_client_free>>;

class Heartbeat {
  public:
    // Factory: connect via Provider
    static Result<Heartbeat, FfiError> connect(Provider& provider);

    // Factory: wrap an existing Idevice socket (consumes it on success)
    static Result<Heartbeat, FfiError> from_socket(Idevice&& socket);

    // Ops
    Result<void, FfiError>             send_polo();
    Result<u_int64_t, FfiError>        get_marco(u_int64_t interval);

    // RAII / moves
    ~Heartbeat() noexcept                              = default;
    Heartbeat(Heartbeat&&) noexcept                    = default;
    Heartbeat& operator=(Heartbeat&&) noexcept         = default;
    Heartbeat(const Heartbeat&)                        = delete;
    Heartbeat&             operator=(const Heartbeat&) = delete;

    HeartbeatClientHandle* raw() const noexcept { return handle_.get(); }
    static Heartbeat       adopt(HeartbeatClientHandle* h) noexcept { return Heartbeat(h); }

  private:
    explicit Heartbeat(HeartbeatClientHandle* h) noexcept : handle_(h) {}
    HeartbeatPtr handle_{};
};

} // namespace IdeviceFFI
