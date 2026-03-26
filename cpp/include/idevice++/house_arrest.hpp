// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <memory>
#include <string>

namespace IdeviceFFI {

class AfcClient; // forward declaration

using HouseArrestPtr =
    std::unique_ptr<HouseArrestClientHandle,
                    FnDeleter<HouseArrestClientHandle, house_arrest_client_free>>;

class HouseArrest {
  public:
    // Factory: connect via Provider
    static Result<HouseArrest, FfiError> connect(Provider& provider);

    // Factory: wrap an existing Idevice socket (consumes it on success)
    static Result<HouseArrest, FfiError> from_socket(Idevice&& socket);

    // Ops - these consume the HouseArrest client and return an AfcClient
    Result<AfcClientHandle*, FfiError>   vend_container(const std::string& bundle_id);
    Result<AfcClientHandle*, FfiError>   vend_documents(const std::string& bundle_id);

    // RAII / moves
    ~HouseArrest() noexcept                                = default;
    HouseArrest(HouseArrest&&) noexcept                    = default;
    HouseArrest& operator=(HouseArrest&&) noexcept         = default;
    HouseArrest(const HouseArrest&)                        = delete;
    HouseArrest&             operator=(const HouseArrest&) = delete;

    HouseArrestClientHandle* raw() const noexcept { return handle_.get(); }
    static HouseArrest       adopt(HouseArrestClientHandle* h) noexcept { return HouseArrest(h); }

  private:
    explicit HouseArrest(HouseArrestClientHandle* h) noexcept : handle_(h) {}
    HouseArrestPtr handle_{};
};

} // namespace IdeviceFFI
