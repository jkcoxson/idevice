// Jackson Coxson

#include <idevice++/remote_pairing_lockdown.hpp>

namespace IdeviceFFI {

// ---- Factories ----
Result<RemotePairingLockdown, FfiError>
RemotePairingLockdown::connect(Provider& provider, const std::string& sending_host) {
    RemotePairingLockdownHandle* out = nullptr;
    if (IdeviceFfiError* e =
            ::remote_pairing_lockdown_connect(provider.raw(), sending_host.c_str(), &out)) {
        return Err(FfiError(e));
    }
    return Ok(RemotePairingLockdown::adopt(out));
}

Result<RemotePairingLockdown, FfiError>
RemotePairingLockdown::from_socket(Idevice&& socket, const std::string& sending_host) {
    RemotePairingLockdownHandle* out = nullptr;
    // Rust consumes the socket regardless of result → release BEFORE call
    if (IdeviceFfiError* e =
            ::remote_pairing_lockdown_new(socket.release(), sending_host.c_str(), &out)) {
        return Err(FfiError(e));
    }
    return Ok(RemotePairingLockdown::adopt(out));
}

// ---- API impls ----
Result<plist_t, FfiError> RemotePairingLockdown::attempt_pair_verify() {
    plist_t handshake = nullptr;
    if (IdeviceFfiError* e =
            ::remote_pairing_lockdown_attempt_pair_verify(handle_.get(), &handshake)) {
        return Err(FfiError(e));
    }
    return Ok(handshake);
}

Result<void, FfiError> RemotePairingLockdown::validate_pairing(RpPairingFile& pairing_file) {
    if (IdeviceFfiError* e =
            ::remote_pairing_lockdown_validate_pairing(handle_.get(), pairing_file.raw())) {
        return Err(FfiError(e));
    }
    return Ok();
}

Result<void, FfiError> RemotePairingLockdown::pair(RpPairingFile&             pairing_file,
                                                   const Option<std::string>& pin) {
    if (IdeviceFfiError* e =
            ::remote_pairing_lockdown_pair(handle_.get(),
                                           pairing_file.raw(),
                                           pin.is_some() ? pin.unwrap().c_str() : nullptr)) {
        return Err(FfiError(e));
    }
    return Ok();
}

Result<void, FfiError> RemotePairingLockdown::connect_pairing(RpPairingFile&             pairing_file,
                                                              const Option<std::string>& pin) {
    if (IdeviceFfiError* e = ::remote_pairing_lockdown_connect_pairing(
            handle_.get(),
            pairing_file.raw(),
            pin.is_some() ? pin.unwrap().c_str() : nullptr)) {
        return Err(FfiError(e));
    }
    return Ok();
}

Result<std::vector<uint8_t>, FfiError> RemotePairingLockdown::encryption_key() const {
    uint8_t* key = nullptr;
    size_t   n   = 0;
    if (IdeviceFfiError* e = ::remote_pairing_lockdown_encryption_key(handle_.get(), &key, &n)) {
        return Err(FfiError(e));
    }

    std::vector<uint8_t> out;
    if (key && n) {
        out.assign(key, key + n);
    }
    ::idevice_data_free(key, n);
    return Ok(std::move(out));
}

} // namespace IdeviceFFI
