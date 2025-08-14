// Jackson Coxson

#include <idevice++/idevice.hpp>

namespace IdeviceFFI {

std::optional<Idevice>
Idevice::create(IdeviceSocketHandle* socket, const std::string& label, FfiError& err) {
    IdeviceHandle* h = nullptr;
    if (IdeviceFfiError* e = idevice_new(socket, label.c_str(), &h)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return Idevice(h);
}

std::optional<Idevice> Idevice::create_tcp(const sockaddr*    addr,
                                           socklen_t          addr_len,
                                           const std::string& label,
                                           FfiError&          err) {
    IdeviceHandle* h = nullptr;
    if (IdeviceFfiError* e = idevice_new_tcp_socket(addr, addr_len, label.c_str(), &h)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return Idevice(h);
}

std::optional<std::string> Idevice::get_type(FfiError& err) const {
    char* cstr = nullptr;
    if (IdeviceFfiError* e = idevice_get_type(handle_.get(), &cstr)) {
        err = FfiError(e);
        return std::nullopt;
    }
    std::string out(cstr);
    idevice_string_free(cstr);
    return out;
}

bool Idevice::rsd_checkin(FfiError& err) {
    if (IdeviceFfiError* e = idevice_rsd_checkin(handle_.get())) {
        err = FfiError(e);
        return false;
    }
    return true;
}

bool Idevice::start_session(const PairingFile& pairing_file, FfiError& err) {
    if (IdeviceFfiError* e = idevice_start_session(handle_.get(), pairing_file.raw())) {
        err = FfiError(e);
        return false;
    }
    return true;
}

} // namespace IdeviceFFI
