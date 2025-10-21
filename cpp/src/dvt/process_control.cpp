// Jackson Coxson

#include <idevice++/dvt/process_control.hpp>

namespace IdeviceFFI {

Result<ProcessControl, FfiError> ProcessControl::create(RemoteServer& server) {
    ProcessControlHandle* out = nullptr;
    FfiError              e(::process_control_new(server.raw(), &out));
    if (e) {
        return Err(e);
    }
    return Ok(ProcessControl::adopt(out));
}

Result<u_int64_t, FfiError> ProcessControl::launch_app(std::string                      bundle_id,
                                                       Option<std::vector<std::string>> env_vars,
                                                       Option<std::vector<std::string>> arguments,
                                                       bool start_suspended,
                                                       bool kill_existing) {
    std::vector<const char*> c_env_vars;
    size_t                   env_vars_len = 0;
    if (env_vars.is_some()) {
        c_env_vars.reserve(env_vars.unwrap().size());
        for (auto& a : env_vars.unwrap()) {
            c_env_vars.push_back(a.c_str());
        }
    }

    std::vector<const char*> c_arguments;
    size_t                   arguments_len = 0;
    if (arguments.is_some()) {
        c_arguments.reserve(arguments.unwrap().size());
        for (auto& a : arguments.unwrap()) {
            c_arguments.push_back(a.c_str());
        }
    }

    u_int64_t pid = 0;

    FfiError  e(::process_control_launch_app(
        handle_.get(),
        bundle_id.c_str(),
        c_env_vars.empty() ? nullptr : const_cast<const char* const*>(c_env_vars.data()),
        env_vars_len,
        c_arguments.empty() ? nullptr : const_cast<const char* const*>(c_arguments.data()),
        arguments_len,
        start_suspended,
        kill_existing,
        &pid));
    if (e) {
        return Err(e);
    }
    return Ok(pid);
}

Result<void, FfiError> ProcessControl::kill_app(u_int64_t pid) {
    FfiError e(::process_control_kill_app(handle_.get(), pid));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> ProcessControl::disable_memory_limit(u_int64_t pid) {
    FfiError e(::process_control_disable_memory_limit(handle_.get(), pid));
    if (e) {
        return Err(e);
    }
    return Ok();
}

} // namespace IdeviceFFI
