// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/idevice.hpp>
#include <idevice++/result.hpp>
#include <memory>
#include <string>
#include <vector>

namespace IdeviceFFI {

using RpPairingFilePtr =
    std::unique_ptr<RpPairingFileHandle, FnDeleter<RpPairingFileHandle, rp_pairing_file_free>>;

class RpPairingFile {
  public:
    // Factory: generate a new pairing file with fresh Ed25519 keys
    static Result<RpPairingFile, FfiError> generate(const std::string& hostname);

    // Factory: read from a file path
    static Result<RpPairingFile, FfiError> from_file(const std::string& path);

    // Factory: parse from plist bytes (XML or binary)
    static Result<RpPairingFile, FfiError> from_bytes(const uint8_t* data, size_t len);

    // Serialize to XML plist bytes
    Result<std::vector<uint8_t>, FfiError> to_bytes() const;

    // Write to a file path
    Result<void, FfiError>                 write(const std::string& path) const;

    // RAII / moves
    ~RpPairingFile() noexcept                                = default;
    RpPairingFile(RpPairingFile&&) noexcept                  = default;
    RpPairingFile& operator=(RpPairingFile&&) noexcept       = default;
    RpPairingFile(const RpPairingFile&)                      = delete;
    RpPairingFile&        operator=(const RpPairingFile&)    = delete;

    RpPairingFileHandle*  raw() const noexcept { return handle_.get(); }
    static RpPairingFile  adopt(RpPairingFileHandle* h) noexcept { return RpPairingFile(h); }

  private:
    explicit RpPairingFile(RpPairingFileHandle* h) noexcept : handle_(h) {}
    RpPairingFilePtr handle_{};
};

} // namespace IdeviceFFI
