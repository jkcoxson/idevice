// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/dvt/remote_server.hpp>
#include <idevice++/result.hpp>
#include <memory>
#include <string>

namespace IdeviceFFI {

using GraphicsPtr =
    std::unique_ptr<GraphicsHandle, FnDeleter<GraphicsHandle, graphics_free>>;

struct GraphicsSample {
    uint64_t    timestamp                   = 0;
    double      fps                         = 0.0;
    uint64_t    alloc_system_memory         = 0;
    uint64_t    in_use_system_memory        = 0;
    uint64_t    in_use_system_memory_driver = 0;
    std::string gpu_bundle_name;
    uint64_t    recovery_count              = 0;
};

class Graphics {
  public:
    static Result<Graphics, FfiError> create(RemoteServer& server);

    Result<void, FfiError>           start(double interval);
    Result<void, FfiError>           stop();
    Result<GraphicsSample, FfiError> next_sample();

    ~Graphics() noexcept                         = default;
    Graphics(Graphics&&) noexcept                = default;
    Graphics& operator=(Graphics&&) noexcept     = default;
    Graphics(const Graphics&)                    = delete;
    Graphics&       operator=(const Graphics&)   = delete;

    GraphicsHandle* raw() const noexcept { return handle_.get(); }
    static Graphics adopt(GraphicsHandle* h) noexcept { return Graphics(h); }

  private:
    explicit Graphics(GraphicsHandle* h) noexcept : handle_(h) {}
    GraphicsPtr handle_{};
};

} // namespace IdeviceFFI
