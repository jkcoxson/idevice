// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <memory>
#include <vector>

namespace IdeviceFFI {

using OsTraceRelayPtr = std::unique_ptr<OsTraceRelayClientHandle,
                                        FnDeleter<OsTraceRelayClientHandle, os_trace_relay_free>>;

using OsTraceRelayReceiverPtr =
    std::unique_ptr<OsTraceRelayReceiverHandle,
                    FnDeleter<OsTraceRelayReceiverHandle, os_trace_relay_receiver_free>>;

class OsTraceRelayReceiver;

class OsTraceRelay {
  public:
    // Factory: connect via Provider
    static Result<OsTraceRelay, FfiError>   connect(Provider& provider);

    // Ops
    Result<OsTraceRelayReceiver, FfiError>  start_trace(const uint32_t* pid = nullptr);
    Result<std::vector<uint64_t>, FfiError> get_pid_list();

    // RAII / moves
    ~OsTraceRelay() noexcept                                 = default;
    OsTraceRelay(OsTraceRelay&&) noexcept                    = default;
    OsTraceRelay& operator=(OsTraceRelay&&) noexcept         = default;
    OsTraceRelay(const OsTraceRelay&)                        = delete;
    OsTraceRelay&             operator=(const OsTraceRelay&) = delete;

    OsTraceRelayClientHandle* raw() const noexcept { return handle_.get(); }
    static OsTraceRelay adopt(OsTraceRelayClientHandle* h) noexcept { return OsTraceRelay(h); }
    OsTraceRelayClientHandle* release() noexcept { return handle_.release(); }

  private:
    explicit OsTraceRelay(OsTraceRelayClientHandle* h) noexcept : handle_(h) {}
    OsTraceRelayPtr handle_{};
};

class OsTraceRelayReceiver {
  public:
    // Ops
    Result<OsTraceLog*, FfiError> next();

    // Free a log obtained from next()
    static void                   free_log(OsTraceLog* log);

    // RAII / moves
    ~OsTraceRelayReceiver() noexcept                                   = default;
    OsTraceRelayReceiver(OsTraceRelayReceiver&&) noexcept              = default;
    OsTraceRelayReceiver& operator=(OsTraceRelayReceiver&&) noexcept   = default;
    OsTraceRelayReceiver(const OsTraceRelayReceiver&)                  = delete;
    OsTraceRelayReceiver&       operator=(const OsTraceRelayReceiver&) = delete;

    OsTraceRelayReceiverHandle* raw() const noexcept { return handle_.get(); }
    static OsTraceRelayReceiver adopt(OsTraceRelayReceiverHandle* h) noexcept {
        return OsTraceRelayReceiver(h);
    }

  private:
    explicit OsTraceRelayReceiver(OsTraceRelayReceiverHandle* h) noexcept : handle_(h) {}
    OsTraceRelayReceiverPtr handle_{};
};

} // namespace IdeviceFFI
