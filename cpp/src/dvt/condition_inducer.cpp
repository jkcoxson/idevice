// Jackson Coxson

#include <idevice++/dvt/condition_inducer.hpp>

namespace IdeviceFFI {

Result<ConditionInducer, FfiError> ConditionInducer::create(RemoteServer& server) {
    ConditionInducerHandle* out = nullptr;
    FfiError                e(::condition_inducer_new(server.raw(), &out));
    if (e) return Err(e);
    return Ok(ConditionInducer::adopt(out));
}

Result<std::vector<ConditionGroup>, FfiError> ConditionInducer::available_conditions() {
    IdeviceConditionGroup** ptrs  = nullptr;
    size_t                  count = 0;
    FfiError                e(::condition_inducer_available_conditions(handle_.get(), &ptrs, &count));
    if (e) return Err(e);

    std::vector<ConditionGroup> result;
    result.reserve(count);
    for (size_t i = 0; i < count; ++i) {
        auto* g = ptrs[i];
        ConditionGroup cg;
        cg.identifier = g->identifier ? std::string(g->identifier) : std::string();
        for (size_t j = 0; j < g->profiles_count; ++j) {
            auto& p = g->profiles[j];
            cg.profiles.push_back({
                p.identifier  ? std::string(p.identifier)  : std::string(),
                p.description ? std::string(p.description) : std::string(),
            });
        }
        result.push_back(std::move(cg));
    }
    ::condition_inducer_groups_free(ptrs, count);
    return Ok(std::move(result));
}

Result<void, FfiError> ConditionInducer::enable(const std::string& condition_id,
                                                 const std::string& profile_id) {
    FfiError e(::condition_inducer_enable(handle_.get(), condition_id.c_str(),
                                          profile_id.c_str()));
    if (e) return Err(e);
    return Ok();
}

Result<void, FfiError> ConditionInducer::disable() {
    FfiError e(::condition_inducer_disable(handle_.get()));
    if (e) return Err(e);
    return Ok();
}

} // namespace IdeviceFFI
