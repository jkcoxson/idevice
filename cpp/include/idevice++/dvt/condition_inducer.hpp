// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/dvt/remote_server.hpp>
#include <idevice++/result.hpp>
#include <memory>
#include <string>
#include <vector>

namespace IdeviceFFI {

using ConditionInducerPtr =
    std::unique_ptr<ConditionInducerHandle,
                    FnDeleter<ConditionInducerHandle, condition_inducer_free>>;

/// A single condition profile within a group
struct ConditionProfile {
    std::string identifier;
    std::string description;
};

/// A condition inducer group containing one or more profiles
struct ConditionGroup {
    std::string                  identifier;
    std::vector<ConditionProfile> profiles;
};

class ConditionInducer {
  public:
    static Result<ConditionInducer, FfiError> create(RemoteServer& server);

    Result<std::vector<ConditionGroup>, FfiError> available_conditions();
    Result<void, FfiError> enable(const std::string& condition_id,
                                  const std::string& profile_id);
    Result<void, FfiError> disable();

    ~ConditionInducer() noexcept                               = default;
    ConditionInducer(ConditionInducer&&) noexcept              = default;
    ConditionInducer& operator=(ConditionInducer&&) noexcept   = default;
    ConditionInducer(const ConditionInducer&)                  = delete;
    ConditionInducer&       operator=(const ConditionInducer&) = delete;

    ConditionInducerHandle* raw() const noexcept { return handle_.get(); }
    static ConditionInducer adopt(ConditionInducerHandle* h) noexcept {
        return ConditionInducer(h);
    }

  private:
    explicit ConditionInducer(ConditionInducerHandle* h) noexcept : handle_(h) {}
    ConditionInducerPtr handle_{};
};

} // namespace IdeviceFFI
