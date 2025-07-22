// Jackson Coxson

#ifndef IDEVICE_PAIRING_FILE
#define IDEVICE_PAIRING_FILE

#pragma once

#include "ffi.hpp"
#include <optional>
#include <string>
#include <vector>

namespace IdeviceFFI {
class PairingFile {
  public:
    static std::optional<PairingFile> read(const std::string& path, FfiError& err) {
        IdevicePairingFile* ptr = nullptr;
        IdeviceFfiError*    e   = idevice_pairing_file_read(path.c_str(), &ptr);
        if (e) {
            err = FfiError::from(e);
            return std::nullopt;
        }
        return PairingFile(ptr);
    }

    static std::optional<PairingFile> from_bytes(const uint8_t* data, size_t size, FfiError& err) {
        IdevicePairingFile* raw = nullptr;
        IdeviceFfiError*    e   = idevice_pairing_file_from_bytes(data, size, &raw);
        if (e) {
            err = FfiError::from(e);
            return std::nullopt;
        }
        return PairingFile(raw);
    }

    ~PairingFile() {
        if (ptr_) {
            idevice_pairing_file_free(ptr_);
        }
    }

    // Deleted copy constructor and assignment — use unique ownersship
    PairingFile(const PairingFile&)            = delete;
    PairingFile& operator=(const PairingFile&) = delete;

    // Move constructor and assignment
    PairingFile(PairingFile&& other) noexcept : ptr_(other.ptr_) { other.ptr_ = nullptr; }

    PairingFile& operator=(PairingFile&& other) noexcept {
        if (this != &other) {
            if (ptr_) {
                idevice_pairing_file_free(ptr_);
            }
            ptr_       = other.ptr_;
            other.ptr_ = nullptr;
        }
        return *this;
    }

    std::optional<std::vector<uint8_t>> serialize(FfiError& err) const {
        uint8_t*         data = nullptr;
        size_t           size = 0;
        IdeviceFfiError* e    = idevice_pairing_file_serialize(ptr_, &data, &size);
        if (e) {
            err = FfiError::from(e);
            return std::nullopt;
        }

        std::vector<uint8_t> result(data, data + size);
        delete[] data; // NOTE: adjust this if deallocation uses `free` or a custom function
        return result;
    }

    explicit PairingFile(IdevicePairingFile* ptr) : ptr_(ptr) {}
    IdevicePairingFile* raw() const { return ptr_; }

  private:
    IdevicePairingFile* ptr_;
};

} // namespace IdeviceFFI
#endif
