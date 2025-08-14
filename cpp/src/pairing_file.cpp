// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/pairing_file.hpp>

namespace IdeviceFFI {

// Deleter definition (out-of-line)
void PairingFileDeleter::operator()(IdevicePairingFile* p) const noexcept {
    if (p)
        idevice_pairing_file_free(p);
}

// Static member definitions
std::optional<PairingFile> PairingFile::read(const std::string& path, FfiError& err) {
    IdevicePairingFile* ptr = nullptr;
    if (IdeviceFfiError* e = idevice_pairing_file_read(path.c_str(), &ptr)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return PairingFile(ptr);
}

std::optional<PairingFile>
PairingFile::from_bytes(const uint8_t* data, size_t size, FfiError& err) {
    IdevicePairingFile* raw = nullptr;
    if (IdeviceFfiError* e = idevice_pairing_file_from_bytes(data, size, &raw)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return PairingFile(raw);
}

std::optional<std::vector<uint8_t>> PairingFile::serialize(FfiError& err) const {
    if (!ptr_) {
        return std::nullopt;
    }

    uint8_t* data = nullptr;
    size_t   size = 0;

    if (IdeviceFfiError* e = idevice_pairing_file_serialize(ptr_.get(), &data, &size)) {
        err = FfiError(e);
        return std::nullopt;
    }

    std::vector<uint8_t> out(data, data + size);
    idevice_data_free(data, size);
    return out;
}

} // namespace IdeviceFFI
