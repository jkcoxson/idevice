// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <memory>
#include <string>

namespace IdeviceFFI {

using MobileActivationdPtr =
    std::unique_ptr<MobileActivationdClientHandle,
                    FnDeleter<MobileActivationdClientHandle, mobileactivationd_client_free>>;

class MobileActivationd {
  public:
    // Factory: connect via Provider
    static Result<MobileActivationd, FfiError> connect(Provider& provider);

    // Ops
    Result<std::string, FfiError>              get_state();
    Result<bool, FfiError>                     is_activated();
    Result<void, FfiError>                     deactivate();

    // RAII / moves
    ~MobileActivationd() noexcept                                      = default;
    MobileActivationd(MobileActivationd&&) noexcept                    = default;
    MobileActivationd& operator=(MobileActivationd&&) noexcept         = default;
    MobileActivationd(const MobileActivationd&)                        = delete;
    MobileActivationd&             operator=(const MobileActivationd&) = delete;

    MobileActivationdClientHandle* raw() const noexcept { return handle_.get(); }
    static MobileActivationd       adopt(MobileActivationdClientHandle* h) noexcept {
        return MobileActivationd(h);
    }

  private:
    explicit MobileActivationd(MobileActivationdClientHandle* h) noexcept : handle_(h) {}
    MobileActivationdPtr handle_{};
};

} // namespace IdeviceFFI
