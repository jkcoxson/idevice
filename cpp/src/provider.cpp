// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>

namespace IdeviceFFI {

Result<Provider, FfiError>
Provider::tcp_new(const idevice_sockaddr* ip, PairingFile&& pairing, const std::string& label) {
    IdeviceProviderHandle* out = nullptr;

    FfiError               e(idevice_tcp_provider_new(
        ip, static_cast<IdevicePairingFile*>(pairing.raw()), label.c_str(), &out));
    if (e) {
        return Err(e);
    }

    // Success: Rust consumed the pairing file -> abandon our ownership
    pairing.release();

    return Ok(Provider::adopt(out));
}

Result<Provider, FfiError> Provider::usbmuxd_new(UsbmuxdAddr&&      addr,
                                                 uint32_t           tag,
                                                 const std::string& udid,
                                                 uint32_t           device_id,
                                                 const std::string& label) {
    IdeviceProviderHandle* out = nullptr;

    FfiError               e(usbmuxd_provider_new(static_cast<UsbmuxdAddrHandle*>(addr.raw()),
                                    tag,
                                    udid.c_str(),
                                    device_id,
                                    label.c_str(),
                                    &out));
    if (e) {
        return Err(e);
    }

    // Success: Rust consumed the addr -> abandon our ownership
    addr.release();
    return Ok(Provider::adopt(out));
}

Result<PairingFile, FfiError> Provider::get_pairing_file() {

    IdevicePairingFile* out = nullptr;
    FfiError            e(idevice_provider_get_pairing_file(handle_.get(), &out));
    if (e) {
        return Err(e);
    }

    return Ok(PairingFile(out));
}

} // namespace IdeviceFFI
