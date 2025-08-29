
#pragma once
#include <cstdint>
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <memory>
#include <string>

namespace IdeviceFFI {

using LockdownPtr =
    std::unique_ptr<LockdowndClientHandle, FnDeleter<LockdowndClientHandle, lockdownd_client_free>>;

class Lockdown {
  public:
    // Factory: connect via Provider
    static Result<Lockdown, FfiError>           connect(Provider& provider);

    // Factory: wrap an existing Idevice socket (consumes it on success)
    static Result<Lockdown, FfiError>           from_socket(Idevice&& socket);

    // Ops
    Result<void, FfiError>                      start_session(const PairingFile& pf);
    Result<std::pair<uint16_t, bool>, FfiError> start_service(const std::string& identifier);
    Result<plist_t, FfiError>                   get_value(const char* key, const char* domain);

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
