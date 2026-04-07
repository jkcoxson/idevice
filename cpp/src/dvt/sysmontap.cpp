// Jackson Coxson

#include <idevice++/dvt/sysmontap.hpp>

namespace IdeviceFFI {

Result<Sysmontap, FfiError> Sysmontap::create(RemoteServer& server) {
    SysmontapHandle* out = nullptr;
    FfiError         e(::sysmontap_new(server.raw(), &out));
    if (e) return Err(e);
    return Ok(Sysmontap::adopt(out));
}

Result<void, FfiError> Sysmontap::set_config(
    uint32_t                        interval_ms,
    const std::vector<std::string>& process_attributes,
    const std::vector<std::string>& system_attributes) {

    std::vector<const char*> c_proc;
    c_proc.reserve(process_attributes.size());
    for (auto& s : process_attributes) c_proc.push_back(s.c_str());

    std::vector<const char*> c_sys;
    c_sys.reserve(system_attributes.size());
    for (auto& s : system_attributes) c_sys.push_back(s.c_str());

    IdeviceSysmontapConfig cfg{};
    cfg.interval_ms              = interval_ms;
    cfg.process_attributes       = c_proc.empty() ? nullptr : c_proc.data();
    cfg.process_attributes_count = c_proc.size();
    cfg.system_attributes        = c_sys.empty()  ? nullptr : c_sys.data();
    cfg.system_attributes_count  = c_sys.size();

    FfiError e(::sysmontap_set_config(handle_.get(), &cfg));
    if (e) return Err(e);
    return Ok();
}

Result<void, FfiError> Sysmontap::start() {
    FfiError e(::sysmontap_start(handle_.get()));
    if (e) return Err(e);
    return Ok();
}

Result<void, FfiError> Sysmontap::stop() {
    FfiError e(::sysmontap_stop(handle_.get()));
    if (e) return Err(e);
    return Ok();
}

Result<SysmontapSample, FfiError> Sysmontap::next_sample() {
    SysmontapSample sample{};
    FfiError        e(::sysmontap_next_sample(handle_.get(),
                                               &sample.processes,
                                               &sample.system,
                                               &sample.system_cpu_usage));
    if (e) return Err(e);
    return Ok(std::move(sample));
}

} // namespace IdeviceFFI
