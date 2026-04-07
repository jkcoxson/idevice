// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/idevice.hpp>
#include <idevice++/provider.hpp>
#include <memory>
#include <string>

namespace IdeviceFFI {

using LockdownLocationSimulationPtr =
    std::unique_ptr<LocationSimulationServiceHandle,
                    FnDeleter<LocationSimulationServiceHandle, lockdown_location_simulation_free>>;

class LockdownLocationSimulation {
  public:
    // Factory: connect via Provider
    static Result<LockdownLocationSimulation, FfiError> connect(Provider& provider);

    // Factory: wrap an existing Idevice socket (consumes it on success)
    static Result<LockdownLocationSimulation, FfiError> from_socket(Idevice&& socket);

    // Ops
    Result<void, FfiError>                              set(const std::string& latitude,
                                                           const std::string& longitude);
    Result<void, FfiError>                              clear();

    // RAII / moves
    ~LockdownLocationSimulation() noexcept = default;
    LockdownLocationSimulation(LockdownLocationSimulation&&) noexcept = default;
    LockdownLocationSimulation& operator=(LockdownLocationSimulation&&) noexcept = default;
    LockdownLocationSimulation(const LockdownLocationSimulation&) = delete;
    LockdownLocationSimulation& operator=(const LockdownLocationSimulation&) = delete;

    LocationSimulationServiceHandle* raw() const noexcept { return handle_.get(); }
    static LockdownLocationSimulation adopt(LocationSimulationServiceHandle* h) noexcept {
        return LockdownLocationSimulation(h);
    }

  private:
    explicit LockdownLocationSimulation(LocationSimulationServiceHandle* h) noexcept : handle_(h) {
    }
    LockdownLocationSimulationPtr handle_{};
};

} // namespace IdeviceFFI
