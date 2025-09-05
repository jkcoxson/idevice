// Jackson Coxson

#ifndef IDEVICE_PAIRING_FILE
#define IDEVICE_PAIRING_FILE

#pragma once

#include <idevice++/ffi.hpp>
#include <idevice++/result.hpp>
#include <memory>
#include <string>
#include <vector>

namespace IdeviceFFI {
struct PairingFileDeleter {
    void operator()(IdevicePairingFile* p) const noexcept;
};

using PairingFilePtr = std::unique_ptr<IdevicePairingFile, PairingFileDeleter>;

class PairingFile {
  public:
    static Result<PairingFile, FfiError> read(const std::string& path);
    static Result<PairingFile, FfiError> from_bytes(const uint8_t* data, size_t size);

    ~PairingFile() noexcept                    = default; // unique_ptr handles destruction

    PairingFile(const PairingFile&)            = delete;
    PairingFile& operator=(const PairingFile&) = delete;

    PairingFile(PairingFile&&) noexcept        = default; // move is correct by default
    PairingFile&                           operator=(PairingFile&&) noexcept = default;

    Result<std::vector<uint8_t>, FfiError> serialize() const;

    explicit PairingFile(IdevicePairingFile* ptr) noexcept : ptr_(ptr) {}
    IdevicePairingFile* raw() const noexcept { return ptr_.get(); }
    IdevicePairingFile* release() noexcept { return ptr_.release(); }

  private:
    PairingFilePtr ptr_{}; // owns the handle
};

} // namespace IdeviceFFI
#endif
