// Jackson Coxson

#ifndef IDEVICE_CPP
#define IDEVICE_CPP

#include "ffi.hpp"
#include "pairing_file.hpp"
#include <optional>
#include <string>

#if defined(_WIN32) && !defined(__MINGW32__)
// MSVC doesn't have BSD u_int* types
using u_int8_t  = std::uint8_t;
using u_int16_t = std::uint16_t;
using u_int32_t = std::uint32_t;
using u_int64_t = std::uint64_t;
#endif

namespace IdeviceFFI {

class Idevice {
  public:
    static std::optional<Idevice>
    create(IdeviceSocketHandle* socket, const std::string& label, FfiError& err) {
        IdeviceHandle*   handle = nullptr;
        IdeviceFfiError* e      = idevice_new(socket, label.c_str(), &handle);
        if (e) {
            err = FfiError::from(e);
            return std::nullopt;
        }
        return Idevice(handle);
    }

    static std::optional<Idevice>
    create_tcp(const sockaddr* addr, socklen_t addr_len, const std::string& label, FfiError& err) {
        IdeviceHandle*   handle = nullptr;
        IdeviceFfiError* e      = idevice_new_tcp_socket(addr, addr_len, label.c_str(), &handle);
        if (e) {
            err = FfiError::from(e);
            return std::nullopt;
        }
        return Idevice(handle);
    }

    std::optional<std::string> get_type(FfiError& err) {
        char*            type_cstr = nullptr;
        IdeviceFfiError* e         = idevice_get_type(handle_, &type_cstr);
        if (e) {
            err = FfiError::from(e);
            return std::nullopt;
        }

        std::string type_str(type_cstr);
        idevice_string_free(type_cstr);
        return type_str;
    }

    bool rsd_checkin(FfiError& err) {
        IdeviceFfiError* e = idevice_rsd_checkin(handle_);
        if (e) {
            err = FfiError::from(e);
            return false;
        }
        return true;
    }

    bool start_session(PairingFile& pairing_file, FfiError& err) {
        IdeviceFfiError* e = idevice_start_session(handle_, pairing_file.raw());
        if (e) {
            err = FfiError::from(e);
            return false;
        }
        return true;
    }

    ~Idevice() {
        if (handle_)
            idevice_free(handle_);
    }

    explicit Idevice(IdeviceHandle* h) : handle_(h) {}

  private:
    IdeviceHandle* handle_;
};

} // namespace IdeviceFFI
#endif
