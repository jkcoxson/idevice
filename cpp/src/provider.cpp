// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>

namespace IdeviceFFI {

std::optional<Provider> Provider::tcp_new(const idevice_sockaddr* ip,
                                          PairingFile&&           pairing,
                                          const std::string&      label,
                                          FfiError&               err) {
    IdeviceProviderHandle* out = nullptr;

    // Call with exact types; do NOT cast to void*
    if (IdeviceFfiError* e = idevice_tcp_provider_new(
            ip, static_cast<IdevicePairingFile*>(pairing.raw()), label.c_str(), &out)) {
        err = FfiError(e);
        return std::nullopt;
    }

    // Success: Rust consumed the pairing file -> abandon our ownership
    pairing.release(); // implement as: ptr_.release() in PairingFile

    return Provider::adopt(out);
}

std::optional<Provider> Provider::usbmuxd_new(UsbmuxdAddr&&      addr,
                                              uint32_t           tag,
                                              const std::string& udid,
                                              uint32_t           device_id,
                                              const std::string& label,
                                              FfiError&          err) {
    IdeviceProviderHandle* out = nullptr;

    if (IdeviceFfiError* e = usbmuxd_provider_new(static_cast<UsbmuxdAddrHandle*>(addr.raw()),
                                                  tag,
                                                  udid.c_str(),
                                                  device_id,
                                                  label.c_str(),
                                                  &out)) {
        err = FfiError(e);
        return std::nullopt;
    }

    // Success: Rust consumed the addr -> abandon our ownership
    addr.release(); // implement as: ptr_.release() in UsbmuxdAddr

    return Provider::adopt(out);
}

std::optional<PairingFile> Provider::get_pairing_file(FfiError& err) {

    IdevicePairingFile* out = nullptr;
    if (IdeviceFfiError* e = idevice_provider_get_pairing_file(handle_.get(), &out)) {
      err = FfiError(e);
        return std::nullopt;
    }
    
    return PairingFile(out);
}

} // namespace IdeviceFFI
