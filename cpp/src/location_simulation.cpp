// Jackson Coxson

#include <idevice++/location_simulation.hpp>

namespace IdeviceFFI {

std::optional<LocationSimulation> LocationSimulation::create(RemoteServer& server, FfiError& err) {
    LocationSimulationHandle* out = nullptr;
    if (IdeviceFfiError* e = ::location_simulation_new(server.raw(), &out)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return LocationSimulation::adopt(out);
}

bool LocationSimulation::clear(FfiError& err) {
    if (IdeviceFfiError* e = ::location_simulation_clear(handle_.get())) {
        err = FfiError(e);
        return false;
    }
    return true;
}

bool LocationSimulation::set(double latitude, double longitude, FfiError& err) {
    if (IdeviceFfiError* e = ::location_simulation_set(handle_.get(), latitude, longitude)) {
        err = FfiError(e);
        return false;
    }
    return true;
}

} // namespace IdeviceFFI
