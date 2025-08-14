// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/lockdown.hpp>
#include <idevice++/provider.hpp>

namespace IdeviceFFI {

std::optional<Lockdown> Lockdown::connect(Provider& provider, FfiError& err) {
    LockdowndClientHandle* out = nullptr;

    if (IdeviceFfiError* e = ::lockdownd_connect(provider.raw(), &out)) {
        // Rust freed the provider on error -> abandon our ownership to avoid double free.
        // Your Provider wrapper should expose release().
        provider.release();
        err = FfiError(e);
        return std::nullopt;
    }
    // Success: provider is NOT consumed; keep ownership.
    return Lockdown::adopt(out);
}

std::optional<Lockdown> Lockdown::from_socket(Idevice&& socket, FfiError& err) {
    LockdowndClientHandle* out = nullptr;

    if (IdeviceFfiError* e = ::lockdownd_new(socket.raw(), &out)) {
        // Error: Rust did NOT consume the socket (it returns early for invalid args),
        // so keep ownership; report error.
        err = FfiError(e);
        return std::nullopt;
    }
    // Success: Rust consumed the socket -> abandon our ownership.
    socket.release();
    return Lockdown::adopt(out);
}

bool Lockdown::start_session(const PairingFile& pf, FfiError& err) {
    if (IdeviceFfiError* e = ::lockdownd_start_session(handle_.get(), pf.raw())) {
        err = FfiError(e);
        return false;
    }
    return true;
}

std::optional<std::pair<uint16_t, bool>> Lockdown::start_service(const std::string& identifier,
                                                                 FfiError&          err) {
    uint16_t port = 0;
    bool     ssl  = false;
    if (IdeviceFfiError* e =
            ::lockdownd_start_service(handle_.get(), identifier.c_str(), &port, &ssl)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return std::make_pair(port, ssl);
}

std::optional<plist_t> Lockdown::get_value(const char* key, const char* domain, FfiError& err) {
    plist_t out = nullptr;
    if (IdeviceFfiError* e = ::lockdownd_get_value(handle_.get(), key, domain, &out)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return out; // caller now owns `out` and must free with the plist API
}

} // namespace IdeviceFFI
