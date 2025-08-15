// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/remote_server.hpp>
#include <memory>
#include <optional>

namespace IdeviceFFI {

using LocSimPtr = std::unique_ptr<LocationSimulationHandle,
                                  FnDeleter<LocationSimulationHandle, location_simulation_free>>;

class LocationSimulation {
  public:
    // Factory: borrows the RemoteServer; not consumed
    static std::optional<LocationSimulation> create(RemoteServer& server, FfiError& err);

    bool                                     clear(FfiError& err);
    bool                                     set(double latitude, double longitude, FfiError& err);

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
