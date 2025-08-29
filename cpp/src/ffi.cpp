// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice.h>
#include <string>

namespace IdeviceFFI {
FfiError::FfiError(const IdeviceFfiError* err)
    : code(err ? err->code : 0), message(err && err->message ? err->message : "") {
    if (err) {
        idevice_error_free(const_cast<IdeviceFfiError*>(err));
    }
}

FfiError::FfiError() : code(0), message("") {
}

FfiError FfiError::NotConnected() {
    FfiError err;
    err.code    = -11; // from idevice/lib.rs
    err.message = "No established socket connection";
    return err;
}
FfiError FfiError::InvalidArgument() {
    FfiError err;
    err.code    = -57; // from idevice/lib.rs
    err.message = "No established socket connection";
    return err;
}

} // namespace IdeviceFFI
