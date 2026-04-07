// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <memory>
#include <vector>

namespace IdeviceFFI {

using PreboardServicePtr =
    std::unique_ptr<PreboardServiceClientHandle,
                    FnDeleter<PreboardServiceClientHandle, preboard_service_client_free>>;

class PreboardService {
  public:
    // Factory: connect via Provider
    static Result<PreboardService, FfiError> connect(Provider& provider);

    // Factory: connect via RSD tunnel
    static Result<PreboardService, FfiError> connect_rsd(AdapterHandle* adapter, RsdHandshakeHandle* handshake);

    // Factory: wrap an existing Idevice socket (consumes it on success)
    static Result<PreboardService, FfiError> from_socket(Idevice&& socket);

    // Ops
    Result<void, FfiError>                   create_stashbag(const std::vector<uint8_t>& manifest);
    Result<void, FfiError>                   commit_stashbag(const std::vector<uint8_t>& manifest);

    // RAII / moves
    ~PreboardService() noexcept                                    = default;
    PreboardService(PreboardService&&) noexcept                    = default;
    PreboardService& operator=(PreboardService&&) noexcept         = default;
    PreboardService(const PreboardService&)                        = delete;
    PreboardService&             operator=(const PreboardService&) = delete;

    PreboardServiceClientHandle* raw() const noexcept { return handle_.get(); }
    static PreboardService       adopt(PreboardServiceClientHandle* h) noexcept {
        return PreboardService(h);
    }

  private:
    explicit PreboardService(PreboardServiceClientHandle* h) noexcept : handle_(h) {}
    PreboardServicePtr handle_{};
};

} // namespace IdeviceFFI
