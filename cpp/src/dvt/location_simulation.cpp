// Jackson Coxson

#include <idevice++/dvt/location_simulation.hpp>

namespace IdeviceFFI {

Result<LocationSimulation, FfiError> LocationSimulation::create(RemoteServer& server) {
    LocationSimulationHandle* out = nullptr;
    FfiError                  e(::location_simulation_new(server.raw(), &out));
    if (e) {
        return Err(e);
    }
    return Ok(LocationSimulation::adopt(out));
}

Result<void, FfiError> LocationSimulation::clear() {
    FfiError e(::location_simulation_clear(handle_.get()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> LocationSimulation::set(double latitude, double longitude) {
    FfiError e(::location_simulation_set(handle_.get(), latitude, longitude));
    if (e) {
        return Err(e);
    }
    return Ok();
}

} // namespace IdeviceFFI
