// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
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
} // namespace IdeviceFFI
