// Jackson Coxson

#ifndef IDEVICE_RSD_H
#define IDEVICE_RSD_H

#include <idevice++/ffi.hpp>
#include <idevice++/idevice.hpp>
#include <idevice++/readwrite.hpp>
#include <vector>

namespace IdeviceFFI {

struct RsdService {
    std::string              name;
    std::string              entitlement;
    uint16_t                 port{};
    bool                     uses_remote_xpc{};
    std::vector<std::string> features;
    int64_t                  service_version{-1};
};

using RsdPtr =
    std::unique_ptr<RsdHandshakeHandle, FnDeleter<RsdHandshakeHandle, rsd_handshake_free>>;

class RsdHandshake {
  public:
    // Factory: consumes the ReadWrite socket regardless of result
    static Result<RsdHandshake, FfiError>     from_socket(ReadWrite&& rw);

    // Basic info
    Result<size_t, FfiError>                  protocol_version() const;
    Result<std::string, FfiError>             uuid() const;

    // Services
    Result<std::vector<RsdService>, FfiError> services() const;
    Result<bool, FfiError>                    service_available(const std::string& name) const;
    Result<RsdService, FfiError>              service_info(const std::string& name) const;

    // RAII / moves
    ~RsdHandshake() noexcept                         = default;
    RsdHandshake(RsdHandshake&&) noexcept            = default;
    RsdHandshake& operator=(RsdHandshake&&) noexcept = default;

    // Enable Copying
    RsdHandshake(const RsdHandshake& other);
    RsdHandshake&       operator=(const RsdHandshake& other);

    RsdHandshakeHandle* raw() const noexcept { return handle_.get(); }
    static RsdHandshake adopt(RsdHandshakeHandle* h) noexcept { return RsdHandshake(h); }

  private:
    explicit RsdHandshake(RsdHandshakeHandle* h) noexcept : handle_(h) {}
    RsdPtr handle_{};
};

} // namespace IdeviceFFI
#endif
