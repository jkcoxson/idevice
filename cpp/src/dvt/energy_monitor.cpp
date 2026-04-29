// Jackson Coxson

#include <idevice++/dvt/energy_monitor.hpp>

namespace IdeviceFFI {

Result<EnergyMonitor, FfiError> EnergyMonitor::create(RemoteServer& server) {
    EnergyMonitorHandle* out = nullptr;
    FfiError             e(::energy_monitor_new(server.raw(), &out));
    if (e) return Err(e);
    return Ok(EnergyMonitor::adopt(out));
}

Result<void, FfiError> EnergyMonitor::start_sampling(const std::vector<uint32_t>& pids) {
    FfiError e(::energy_monitor_start_sampling(
        handle_.get(), pids.empty() ? nullptr : pids.data(), pids.size()));
    if (e) return Err(e);
    return Ok();
}

Result<void, FfiError> EnergyMonitor::stop_sampling(const std::vector<uint32_t>& pids) {
    FfiError e(::energy_monitor_stop_sampling(
        handle_.get(), pids.empty() ? nullptr : pids.data(), pids.size()));
    if (e) return Err(e);
    return Ok();
}

Result<std::vector<EnergySample>, FfiError> EnergyMonitor::sample_attributes(
    const std::vector<uint32_t>& pids) {
    IdeviceEnergySample* raw   = nullptr;
    size_t               count = 0;
    FfiError             e(::energy_monitor_sample_attributes(
        handle_.get(), pids.empty() ? nullptr : pids.data(), pids.size(), &raw, &count));
    if (e) return Err(e);

    std::vector<EnergySample> samples;
    samples.reserve(count);
    for (size_t i = 0; i < count; ++i) {
        const auto& s = raw[i];
        samples.push_back({
            s.pid,
            s.timestamp,
            s.total_energy,
            s.cpu_energy,
            s.gpu_energy,
            s.networking_energy,
            s.display_energy,
            s.location_energy,
            s.appstate_energy,
        });
    }

    ::energy_monitor_samples_free(raw, count);
    return Ok(std::move(samples));
}

} // namespace IdeviceFFI
