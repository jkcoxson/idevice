// Jackson Coxson

#include <idevice++/idevice.hpp>

namespace IdeviceFFI {

Result<Idevice, FfiError> Idevice::create(IdeviceSocketHandle* socket, const std::string& label) {
    IdeviceHandle* h = nullptr;
    FfiError       e(idevice_new(socket, label.c_str(), &h));
    if (e) {
        return Err(e);
    }
    return Ok(Idevice(h));
}

Result<Idevice, FfiError>
Idevice::create_tcp(const sockaddr* addr, socklen_t addr_len, const std::string& label) {
    IdeviceHandle* h = nullptr;
    FfiError       e(idevice_new_tcp_socket(addr, addr_len, label.c_str(), &h));
    if (e) {
        return Err(e);
    }
    return Ok(Idevice(h));
}

Result<std::string, FfiError> Idevice::get_type() const {
    char*    cstr = nullptr;
    FfiError e(idevice_get_type(handle_.get(), &cstr));
    if (e) {
        return Err(e);
    }
    std::string out(cstr);
    idevice_string_free(cstr);
    return Ok(out);
}

Result<void, FfiError> Idevice::rsd_checkin() {
    FfiError e(idevice_rsd_checkin(handle_.get()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> Idevice::start_session(const PairingFile& pairing_file) {
    FfiError e(idevice_start_session(handle_.get(), pairing_file.raw()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

} // namespace IdeviceFFI
