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
    static std::optional<RsdHandshake>     from_socket(ReadWrite&& rw, FfiError& err);

    // Basic info
    std::optional<size_t>                  protocol_version(FfiError& err) const;
    std::optional<std::string>             uuid(FfiError& err) const;

    // Services
    std::optional<std::vector<RsdService>> services(FfiError& err) const;
    std::optional<bool>       service_available(const std::string& name, FfiError& err) const;
    std::optional<RsdService> service_info(const std::string& name, FfiError& err) const;

    // RAII / moves
    ~RsdHandshake() noexcept                           = default;
    RsdHandshake(RsdHandshake&&) noexcept              = default;
    RsdHandshake& operator=(RsdHandshake&&) noexcept   = default;
    RsdHandshake(const RsdHandshake&)                  = delete;
    RsdHandshake&       operator=(const RsdHandshake&) = delete;

    RsdHandshakeHandle* raw() const noexcept { return handle_.get(); }
    static RsdHandshake adopt(RsdHandshakeHandle* h) noexcept { return RsdHandshake(h); }

  private:
    explicit RsdHandshake(RsdHandshakeHandle* h) noexcept : handle_(h) {}
    RsdPtr handle_{};
};

} // namespace IdeviceFFI
#endif
