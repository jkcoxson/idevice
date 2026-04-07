// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/mobileactivationd.hpp>
#include <idevice++/provider.hpp>

namespace IdeviceFFI {

Result<MobileActivationd, FfiError> MobileActivationd::connect(Provider& provider) {
    MobileActivationdClientHandle* out = nullptr;
    FfiError                       e(::mobileactivationd_connect(provider.raw(), &out));
    if (e) {
        provider.release();
        return Err(e);
    }
    return Ok(MobileActivationd::adopt(out));
}

Result<std::string, FfiError> MobileActivationd::get_state() {
    char*    state = nullptr;
    FfiError e(::mobileactivationd_get_state(handle_.get(), &state));
    if (e) {
        return Err(e);
    }

    std::string result;
    if (state) {
        result = state;
        ::idevice_string_free(state);
    }

    return Ok(std::move(result));
}

Result<bool, FfiError> MobileActivationd::is_activated() {
    bool     activated = false;
    FfiError e(::mobileactivationd_is_activated(handle_.get(), &activated));
    if (e) {
        return Err(e);
    }
    return Ok(activated);
}

Result<void, FfiError> MobileActivationd::deactivate() {
    FfiError e(::mobileactivationd_deactivate(handle_.get()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

} // namespace IdeviceFFI
