// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice.h>
#include <string>

namespace IdeviceFFI {
FfiError::FfiError(const IdeviceFfiError* err)
    : code(err ? err->code : 0),
      sub_code(err ? err->sub_code : 0),
      message(err && err->message ? err->message : "") {
    if (err) {
        idevice_error_free(const_cast<IdeviceFfiError*>(err));
    }
}

FfiError::FfiError() : code(0), sub_code(0), message("") {
}

FfiError FfiError::NotConnected() {
    FfiError err;
    err.code    = 19; // NoEstablishedConnection
    err.message = "No established socket connection";
    return err;
}
FfiError FfiError::InvalidArgument() {
    FfiError err;
    err.code    = 36; // InvalidArgument
    err.message = "Invalid argument";
    return err;
}

} // namespace IdeviceFFI
