// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/pairing_file.hpp>

namespace IdeviceFFI {

// Deleter definition (out-of-line)
void PairingFileDeleter::operator()(IdevicePairingFile* p) const noexcept {
    if (p) {
        idevice_pairing_file_free(p);
    }
}

// Static member definitions
Result<PairingFile, FfiError> PairingFile::read(const std::string& path) {
    IdevicePairingFile* ptr = nullptr;
    FfiError            e(idevice_pairing_file_read(path.c_str(), &ptr));
    if (e) {
        return Err(e);
    }
    return Ok(PairingFile(ptr));
}

Result<PairingFile, FfiError> PairingFile::from_bytes(const uint8_t* data, size_t size) {
    IdevicePairingFile* raw = nullptr;
    FfiError            e(idevice_pairing_file_from_bytes(data, size, &raw));
    if (e) {
        return Err(e);
    }
    return Ok(PairingFile(raw));
}

Result<std::vector<uint8_t>, FfiError> PairingFile::serialize() const {
    if (!ptr_) {
        return Err(FfiError::InvalidArgument());
    }

    uint8_t* data = nullptr;
    size_t   size = 0;

    FfiError e(idevice_pairing_file_serialize(ptr_.get(), &data, &size));
    if (e) {
        return Err(e);
    }

    std::vector<uint8_t> out(data, data + size);
    idevice_data_free(data, size);
    return Ok(out);
}

} // namespace IdeviceFFI
