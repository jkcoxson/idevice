// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/remote_server.hpp>
#include <idevice++/result.hpp>
#include <memory>

namespace IdeviceFFI {

using ProcessControlPtr =
    std::unique_ptr<ProcessControlHandle, FnDeleter<ProcessControlHandle, process_control_free>>;

class ProcessControl {
  public:
    // Factory: borrows the RemoteServer; not consumed
    static Result<ProcessControl, FfiError> create(RemoteServer& server);

    Result<u_int64_t, FfiError>             launch_app(std::string                      bundle_id,
                                                       Option<std::vector<std::string>> env_vars,
                                                       Option<std::vector<std::string>> arguments,
                                                       bool                             start_suspended,
                                                       bool                             kill_existing);
    Result<void, FfiError>                  kill_app(u_int64_t pid);
    Result<void, FfiError>                  disable_memory_limit(u_int64_t pid);

    ~ProcessControl() noexcept                             = default;
    ProcessControl(ProcessControl&&) noexcept              = default;
    ProcessControl& operator=(ProcessControl&&) noexcept   = default;
    ProcessControl(const ProcessControl&)                  = delete;
    ProcessControl&       operator=(const ProcessControl&) = delete;

    ProcessControlHandle* raw() const noexcept { return handle_.get(); }
    static ProcessControl adopt(ProcessControlHandle* h) noexcept { return ProcessControl(h); }

  private:
    explicit ProcessControl(ProcessControlHandle* h) noexcept : handle_(h) {}
    ProcessControlPtr handle_{};
};

} // namespace IdeviceFFI
