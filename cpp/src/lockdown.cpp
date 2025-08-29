// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/lockdown.hpp>
#include <idevice++/provider.hpp>

namespace IdeviceFFI {

Result<Lockdown, FfiError> Lockdown::connect(Provider& provider) {
    LockdowndClientHandle* out = nullptr;
    FfiError               e(::lockdownd_connect(provider.raw(), &out));
    if (e) {
        provider.release();
        return Err(e);
    }
    return Ok(Lockdown::adopt(out));
}

Result<Lockdown, FfiError> Lockdown::from_socket(Idevice&& socket) {
    LockdowndClientHandle* out = nullptr;
    FfiError               e(::lockdownd_new(socket.raw(), &out));
    if (e) {
        return Err(e);
    }
    socket.release();
    return Ok(Lockdown::adopt(out));
}

Result<void, FfiError> Lockdown::start_session(const PairingFile& pf) {
    FfiError e(::lockdownd_start_session(handle_.get(), pf.raw()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<std::pair<uint16_t, bool>, FfiError> Lockdown::start_service(const std::string& identifier) {
    uint16_t port = 0;
    bool     ssl  = false;
    FfiError e(::lockdownd_start_service(handle_.get(), identifier.c_str(), &port, &ssl));
    if (e) {
        return Err(e);
    }
    return Ok(std::make_pair(port, ssl));
}

Result<plist_t, FfiError> Lockdown::get_value(const char* key, const char* domain) {
    plist_t  out = nullptr;
    FfiError e(::lockdownd_get_value(handle_.get(), key, domain, &out));
    if (e) {
        return Err(e);
    }
    return Ok(out);
}

} // namespace IdeviceFFI
