// Jackson Coxson

#include <idevice++/adapter_stream.hpp>

namespace IdeviceFFI {

bool AdapterStream::close(FfiError& err) {
    if (!h_)
        return true;
    if (IdeviceFfiError* e = ::adapter_close(h_)) {
        err = FfiError(e);
        return false;
    }
    h_ = nullptr;
    return true;
}

bool AdapterStream::send(const uint8_t* data, size_t len, FfiError& err) {
    if (!h_)
        return false;
    if (IdeviceFfiError* e = ::adapter_send(h_, data, len)) {
        err = FfiError(e);
        return false;
    }
    return true;
}

bool AdapterStream::recv(std::vector<uint8_t>& out, FfiError& err, size_t max_hint) {
    if (!h_)
        return false;
    if (max_hint == 0)
        max_hint = 2048;
    out.resize(max_hint);
    size_t actual = 0;
    if (IdeviceFfiError* e = ::adapter_recv(h_, out.data(), &actual, out.size())) {
        err = FfiError(e);
        return false;
    }
    out.resize(actual);
    return true;
}

} // namespace IdeviceFFI
