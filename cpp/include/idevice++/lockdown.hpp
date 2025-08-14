
#pragma once
#include <cstdint>
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <memory>
#include <optional>
#include <string>

namespace IdeviceFFI {

using LockdownPtr =
    std::unique_ptr<LockdowndClientHandle, FnDeleter<LockdowndClientHandle, lockdownd_client_free>>;

class Lockdown {
  public:
    // Factory: connect via Provider
    static std::optional<Lockdown>           connect(Provider& provider, FfiError& err);

    // Factory: wrap an existing Idevice socket (consumes it on success)
    static std::optional<Lockdown>           from_socket(Idevice&& socket, FfiError& err);

    // Ops
    bool                                     start_session(const PairingFile& pf, FfiError& err);
    std::optional<std::pair<uint16_t, bool>> start_service(const std::string& identifier,
                                                           FfiError&          err);
    std::optional<plist_t> get_value(const char* key, const char* domain, FfiError& err);

    // RAII / moves
    ~Lockdown() noexcept                              = default;
    Lockdown(Lockdown&&) noexcept                     = default;
    Lockdown& operator=(Lockdown&&) noexcept          = default;
    Lockdown(const Lockdown&)                         = delete;
    Lockdown&              operator=(const Lockdown&) = delete;

    LockdowndClientHandle* raw() const noexcept { return handle_.get(); }
    static Lockdown        adopt(LockdowndClientHandle* h) noexcept { return Lockdown(h); }

  private:
    explicit Lockdown(LockdowndClientHandle* h) noexcept : handle_(h) {}
    LockdownPtr handle_{};
};

} // namespace IdeviceFFI
