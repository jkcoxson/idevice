// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <memory>
#include <string>
#include <vector>

namespace IdeviceFFI {

using MisagentPtr =
    std::unique_ptr<MisagentClientHandle, FnDeleter<MisagentClientHandle, misagent_client_free>>;

class Misagent {
  public:
    // Factory: connect via Provider
    static Result<Misagent, FfiError> connect(Provider& provider);

    // Factory: connect via RSD tunnel
    static Result<Misagent, FfiError> connect_rsd(AdapterHandle* adapter, RsdHandshakeHandle* handshake);

    // Ops
    Result<void, FfiError>            install(const uint8_t* profile_data, size_t profile_len);
    Result<void, FfiError>            remove(const std::string& profile_id);
    Result<std::vector<std::vector<uint8_t>>, FfiError> copy_all();

    // RAII / moves
    ~Misagent() noexcept                             = default;
    Misagent(Misagent&&) noexcept                    = default;
    Misagent& operator=(Misagent&&) noexcept         = default;
    Misagent(const Misagent&)                        = delete;
    Misagent&             operator=(const Misagent&) = delete;

    MisagentClientHandle* raw() const noexcept { return handle_.get(); }
    static Misagent       adopt(MisagentClientHandle* h) noexcept { return Misagent(h); }

  private:
    explicit Misagent(MisagentClientHandle* h) noexcept : handle_(h) {}
    MisagentPtr handle_{};
};

} // namespace IdeviceFFI
