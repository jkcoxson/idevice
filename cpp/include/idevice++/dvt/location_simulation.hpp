// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/dvt/remote_server.hpp>
#include <idevice++/result.hpp>
#include <memory>

namespace IdeviceFFI {

using LocSimPtr = std::unique_ptr<LocationSimulationHandle,
                                  FnDeleter<LocationSimulationHandle, location_simulation_free>>;

class LocationSimulation {
  public:
    // Factory: borrows the RemoteServer; not consumed
    static Result<LocationSimulation, FfiError> create(RemoteServer& server);

    Result<void, FfiError>                      clear();
    Result<void, FfiError>                      set(double latitude, double longitude);

    ~LocationSimulation() noexcept                                 = default;
    LocationSimulation(LocationSimulation&&) noexcept              = default;
    LocationSimulation& operator=(LocationSimulation&&) noexcept   = default;
    LocationSimulation(const LocationSimulation&)                  = delete;
    LocationSimulation&       operator=(const LocationSimulation&) = delete;

    LocationSimulationHandle* raw() const noexcept { return handle_.get(); }
    static LocationSimulation adopt(LocationSimulationHandle* h) noexcept {
        return LocationSimulation(h);
    }

  private:
    explicit LocationSimulation(LocationSimulationHandle* h) noexcept : handle_(h) {}
    LocSimPtr handle_{};
};

} // namespace IdeviceFFI
