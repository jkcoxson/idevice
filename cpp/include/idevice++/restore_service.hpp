// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/idevice.hpp>
#include <idevice++/readwrite.hpp>
#include <idevice++/result.hpp>
#include <memory>
#include <string>

namespace IdeviceFFI {

using RestoreServicePtr =
    std::unique_ptr<RestoreServiceClientHandle,
                    FnDeleter<RestoreServiceClientHandle, restore_service_client_free>>;

class RestoreService {
  public:
    // Factory: from ReadWrite stream (RSD-only, consumes the pointer)
    static Result<RestoreService, FfiError> from_readwrite_ptr(ReadWriteOpaque* consumed);

    // Ergonomic overload: consume a C++ ReadWrite
    static Result<RestoreService, FfiError> from_readwrite(ReadWrite&& rw);

    // Ops
    Result<void, FfiError>                  enter_recovery();
    Result<void, FfiError>                  reboot();
    Result<plist_t, FfiError>               get_preflightinfo();
    Result<plist_t, FfiError>               get_nonces();
    Result<plist_t, FfiError>               get_app_parameters();
    Result<void, FfiError>                  restore_lang(const std::string& language);

    // RAII / moves
    ~RestoreService() noexcept                                   = default;
    RestoreService(RestoreService&&) noexcept                    = default;
    RestoreService& operator=(RestoreService&&) noexcept         = default;
    RestoreService(const RestoreService&)                        = delete;
    RestoreService&             operator=(const RestoreService&) = delete;

    RestoreServiceClientHandle* raw() const noexcept { return handle_.get(); }
    static RestoreService       adopt(RestoreServiceClientHandle* h) noexcept {
        return RestoreService(h);
    }

  private:
    explicit RestoreService(RestoreServiceClientHandle* h) noexcept : handle_(h) {}
    RestoreServicePtr handle_{};
};

} // namespace IdeviceFFI
