// Jackson Coxson

#ifndef IDEVICE_FFI
#define IDEVICE_FFI

#include "idevice.hpp"
#include <string>

namespace IdeviceFFI {
struct FfiError {
    int32_t         code = 0;
    std::string     message;

    static FfiError from(const IdeviceFfiError* err) {
        FfiError out;
        if (err) {
            out.code    = err->code;
            out.message = err->message ? err->message : "";
            idevice_error_free(const_cast<IdeviceFfiError*>(err));
        }
        return out;
    }

    static FfiError success() { return {0, ""}; }

    explicit        operator bool() const { return code != 0; }
};
} // namespace IdeviceFFI
#endif
