// Jackson Coxson

#ifndef IDEVICE_FFI
#define IDEVICE_FFI

#include <idevice++/bindings.hpp>
#include <string>

namespace IdeviceFFI {
class FfiError {
  public:
    int32_t     code = 0;
    std::string message;

    FfiError(const IdeviceFfiError* err);
    FfiError();

    explicit        operator bool() const { return code != 0; }

    static FfiError NotConnected();
    static FfiError InvalidArgument();
};
} // namespace IdeviceFFI
#endif
