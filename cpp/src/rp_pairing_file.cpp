// Jackson Coxson

#include <idevice++/rp_pairing_file.hpp>

namespace IdeviceFFI {

Result<RpPairingFile, FfiError> RpPairingFile::generate(const std::string& hostname) {
    RpPairingFileHandle* out = nullptr;
    FfiError             e(::rp_pairing_file_generate(hostname.c_str(), &out));
    if (e) {
        return Err(e);
    }
    return Ok(RpPairingFile::adopt(out));
}

Result<RpPairingFile, FfiError> RpPairingFile::from_file(const std::string& path) {
    RpPairingFileHandle* out = nullptr;
    FfiError             e(::rp_pairing_file_read(path.c_str(), &out));
    if (e) {
        return Err(e);
    }
    return Ok(RpPairingFile::adopt(out));
}

Result<RpPairingFile, FfiError> RpPairingFile::from_bytes(const uint8_t* data, size_t len) {
    RpPairingFileHandle* out = nullptr;
    FfiError             e(::rp_pairing_file_from_bytes(data, len, &out));
    if (e) {
        return Err(e);
    }
    return Ok(RpPairingFile::adopt(out));
}

Result<std::vector<uint8_t>, FfiError> RpPairingFile::to_bytes() const {
    uint8_t*  data = nullptr;
    uintptr_t len  = 0;
    FfiError  e(::rp_pairing_file_to_bytes(handle_.get(), &data, &len));
    if (e) {
        return Err(e);
    }

    std::vector<uint8_t> result;
    if (data && len > 0) {
        result.assign(data, data + len);
        ::idevice_data_free(data, len);
    }
    return Ok(std::move(result));
}

Result<void, FfiError> RpPairingFile::write(const std::string& path) const {
    FfiError e(::rp_pairing_file_write(handle_.get(), path.c_str()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

} // namespace IdeviceFFI
