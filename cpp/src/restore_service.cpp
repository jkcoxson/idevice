// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/restore_service.hpp>

namespace IdeviceFFI {

Result<RestoreService, FfiError> RestoreService::from_readwrite_ptr(ReadWriteOpaque* consumed) {
    RestoreServiceClientHandle* out = nullptr;
    if (IdeviceFfiError* e = ::restore_service_new(consumed, &out)) {
        return Err(FfiError(e));
    }
    return Ok(RestoreService::adopt(out));
}

Result<RestoreService, FfiError> RestoreService::from_readwrite(ReadWrite&& rw) {
    return from_readwrite_ptr(rw.release());
}

Result<void, FfiError> RestoreService::enter_recovery() {
    FfiError e(::restore_service_enter_recovery(handle_.get()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> RestoreService::reboot() {
    FfiError e(::restore_service_reboot(handle_.get()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<plist_t, FfiError> RestoreService::get_preflightinfo() {
    plist_t  res = nullptr;
    FfiError e(::restore_service_get_preflightinfo(handle_.get(), &res));
    if (e) {
        return Err(e);
    }
    return Ok(res);
}

Result<plist_t, FfiError> RestoreService::get_nonces() {
    plist_t  res = nullptr;
    FfiError e(::restore_service_get_nonces(handle_.get(), &res));
    if (e) {
        return Err(e);
    }
    return Ok(res);
}

Result<plist_t, FfiError> RestoreService::get_app_parameters() {
    plist_t  res = nullptr;
    FfiError e(::restore_service_get_app_parameters(handle_.get(), &res));
    if (e) {
        return Err(e);
    }
    return Ok(res);
}

Result<void, FfiError> RestoreService::restore_lang(const std::string& language) {
    FfiError e(::restore_service_restore_lang(handle_.get(), language.c_str()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

} // namespace IdeviceFFI
