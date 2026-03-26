// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <memory>
#include <string>

namespace IdeviceFFI {

using SyslogRelayPtr =
    std::unique_ptr<SyslogRelayClientHandle,
                    FnDeleter<SyslogRelayClientHandle, syslog_relay_client_free>>;

class SyslogRelay {
  public:
    // Factory: connect via Provider (TCP)
    static Result<SyslogRelay, FfiError> connect_tcp(Provider& provider);

    // Ops
    Result<std::string, FfiError>        next();

    // RAII / moves
    ~SyslogRelay() noexcept                                = default;
    SyslogRelay(SyslogRelay&&) noexcept                    = default;
    SyslogRelay& operator=(SyslogRelay&&) noexcept         = default;
    SyslogRelay(const SyslogRelay&)                        = delete;
    SyslogRelay&             operator=(const SyslogRelay&) = delete;

    SyslogRelayClientHandle* raw() const noexcept { return handle_.get(); }
    static SyslogRelay       adopt(SyslogRelayClientHandle* h) noexcept { return SyslogRelay(h); }

  private:
    explicit SyslogRelay(SyslogRelayClientHandle* h) noexcept : handle_(h) {}
    SyslogRelayPtr handle_{};
};

} // namespace IdeviceFFI
