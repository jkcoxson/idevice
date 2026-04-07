// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/location_simulation.hpp>
#include <idevice++/provider.hpp>

namespace IdeviceFFI {

Result<LockdownLocationSimulation, FfiError>
LockdownLocationSimulation::connect(Provider& provider) {
    LocationSimulationServiceHandle* out = nullptr;
    FfiError e(::lockdown_location_simulation_connect(provider.raw(), &out));
    if (e) {
        provider.release();
        return Err(e);
    }
    return Ok(LockdownLocationSimulation::adopt(out));
}

Result<LockdownLocationSimulation, FfiError>
LockdownLocationSimulation::from_socket(Idevice&& socket) {
    LocationSimulationServiceHandle* out = nullptr;
    FfiError e(::lockdown_location_simulation_new(socket.raw(), &out));
    if (e) {
        return Err(e);
    }
    socket.release();
    return Ok(LockdownLocationSimulation::adopt(out));
}

Result<void, FfiError> LockdownLocationSimulation::set(const std::string& latitude,
                                                       const std::string& longitude) {
    FfiError e(
        ::lockdown_location_simulation_set(handle_.get(), latitude.c_str(), longitude.c_str()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> LockdownLocationSimulation::clear() {
    FfiError e(::lockdown_location_simulation_clear(handle_.get()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

} // namespace IdeviceFFI
