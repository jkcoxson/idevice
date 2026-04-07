// Jackson Coxson

#include <idevice++/amfi.hpp>
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>

namespace IdeviceFFI {

Result<Amfi, FfiError> Amfi::connect(Provider& provider) {
    AmfiClientHandle* out = nullptr;
    FfiError          e(::amfi_connect(provider.raw(), &out));
    if (e) {
        provider.release();
        return Err(e);
    }
    return Ok(Amfi::adopt(out));
}

Result<Amfi, FfiError> Amfi::connect_rsd(AdapterHandle* adapter, RsdHandshakeHandle* handshake) {
    AmfiClientHandle* out = nullptr;
    FfiError          e(::amfi_connect_rsd(adapter, handshake, &out));
    if (e) {
        return Err(e);
    }
    return Ok(Amfi::adopt(out));
}

Result<Amfi, FfiError> Amfi::from_socket(Idevice&& socket) {
    AmfiClientHandle* out = nullptr;
    FfiError          e(::amfi_new(socket.raw(), &out));
    if (e) {
        return Err(e);
    }
    socket.release();
    return Ok(Amfi::adopt(out));
}

Result<void, FfiError> Amfi::reveal_developer_mode_option_in_ui() {
    FfiError e(::amfi_reveal_developer_mode_option_in_ui(handle_.get()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> Amfi::enable_developer_mode() {
    FfiError e(::amfi_enable_developer_mode(handle_.get()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> Amfi::accept_developer_mode() {
    FfiError e(::amfi_accept_developer_mode(handle_.get()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

} // namespace IdeviceFFI
