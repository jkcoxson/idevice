// Jackson Coxson

#pragma once
#include <cstdint>
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/usbmuxd.hpp>
#include <string>

namespace IdeviceFFI {

class FfiError;
class PairingFile; // has: IdevicePairingFile* raw() const; void
                   // release_on_success();
class UsbmuxdAddr; // has: UsbmuxdAddrHandle* raw() const;  void
                   // release_on_success();

using ProviderPtr =
    std::unique_ptr<IdeviceProviderHandle, FnDeleter<IdeviceProviderHandle, idevice_provider_free>>;

class Provider {
  public:
    static Result<Provider, FfiError>
    tcp_new(const idevice_sockaddr* ip, PairingFile&& pairing, const std::string& label);

    static Result<Provider, FfiError> usbmuxd_new(UsbmuxdAddr&&      addr,
                                                  uint32_t           tag,
                                                  const std::string& udid,
                                                  uint32_t           device_id,
                                                  const std::string& label);

    ~Provider() noexcept                                     = default;
    Provider(Provider&&) noexcept                            = default;
    Provider& operator=(Provider&&) noexcept                 = default;
    Provider(const Provider&)                                = delete;
    Provider&                     operator=(const Provider&) = delete;

    Result<PairingFile, FfiError> get_pairing_file();

    IdeviceProviderHandle*        raw() const noexcept { return handle_.get(); }
    static Provider               adopt(IdeviceProviderHandle* h) noexcept { return Provider(h); }
    IdeviceProviderHandle*        release() noexcept { return handle_.release(); }

  private:
    explicit Provider(IdeviceProviderHandle* h) noexcept : handle_(h) {}
    ProviderPtr handle_{};
};

} // namespace IdeviceFFI
