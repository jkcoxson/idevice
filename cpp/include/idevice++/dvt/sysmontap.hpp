// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/dvt/remote_server.hpp>
#include <idevice++/result.hpp>
#include <memory>
#include <string>
#include <vector>

namespace IdeviceFFI {

using SysmontapPtr =
    std::unique_ptr<SysmontapHandle, FnDeleter<SysmontapHandle, sysmontap_free>>;

/// A sysmontap sample containing optional plist_t payloads.
/// Non-null plists must be freed by the caller with plist_free().
struct SysmontapSample {
    /// Per-process data dict (pid → attribute array). May be null.
    plist_t processes       = nullptr;
    /// System attribute array (order matches config's system_attributes). May be null.
    plist_t system          = nullptr;
    /// CPU usage summary dict. May be null.
    plist_t system_cpu_usage = nullptr;
};

class Sysmontap {
  public:
    static Result<Sysmontap, FfiError> create(RemoteServer& server);

    /// Send configuration. Call before start().
    Result<void, FfiError> set_config(uint32_t                        interval_ms,
                                      const std::vector<std::string>& process_attributes,
                                      const std::vector<std::string>& system_attributes);
    /// Starts sampling; consumes the device's initial ack internally.
    Result<void, FfiError>          start();
    Result<void, FfiError>          stop();
    /// Blocks until the next data row arrives.
    Result<SysmontapSample, FfiError> next_sample();

    ~Sysmontap() noexcept                         = default;
    Sysmontap(Sysmontap&&) noexcept               = default;
    Sysmontap& operator=(Sysmontap&&) noexcept    = default;
    Sysmontap(const Sysmontap&)                   = delete;
    Sysmontap&       operator=(const Sysmontap&)  = delete;

    SysmontapHandle* raw() const noexcept { return handle_.get(); }
    static Sysmontap adopt(SysmontapHandle* h) noexcept { return Sysmontap(h); }

  private:
    explicit Sysmontap(SysmontapHandle* h) noexcept : handle_(h) {}
    SysmontapPtr handle_{};
};

} // namespace IdeviceFFI
