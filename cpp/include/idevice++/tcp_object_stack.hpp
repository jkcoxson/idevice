// Jackson Coxson

#pragma once
#include <cstddef>
#include <cstdint>
#include <memory>
#include <optional>
#include <string>

#include <idevice++/core_device_proxy.hpp>
#include <idevice++/ffi.hpp>

namespace IdeviceFFI {

// ---------------- OwnedBuffer: RAII for zero-copy read buffers ----------------
class OwnedBuffer {
  public:
    OwnedBuffer() noexcept : p_(nullptr), n_(0) {}
    OwnedBuffer(const OwnedBuffer&)            = delete;
    OwnedBuffer& operator=(const OwnedBuffer&) = delete;

    OwnedBuffer(OwnedBuffer&& o) noexcept : p_(o.p_), n_(o.n_) {
        o.p_ = nullptr;
        o.n_ = 0;
    }
    OwnedBuffer& operator=(OwnedBuffer&& o) noexcept {
        if (this != &o) {
            reset();
            p_   = o.p_;
            n_   = o.n_;
            o.p_ = nullptr;
            o.n_ = 0;
        }
        return *this;
    }

    ~OwnedBuffer() { reset(); }

    const uint8_t* data() const noexcept { return p_; }
    uint8_t*       data() noexcept { return p_; }
    std::size_t    size() const noexcept { return n_; }
    bool           empty() const noexcept { return n_ == 0; }

    void           reset() noexcept {
        if (p_) {
            ::idevice_data_free(p_, n_);
            p_ = nullptr;
            n_ = 0;
        }
    }

  private:
    friend class TcpObjectStackEater;
    void adopt(uint8_t* p, std::size_t n) noexcept {
        reset();
        p_ = p;
        n_ = n;
    }

    uint8_t*    p_;
    std::size_t n_;
};

// ---------------- TcpFeeder: push inbound IP packets into the stack ----------
class TcpObjectStackFeeder {
  public:
    TcpObjectStackFeeder()                                       = default;
    TcpObjectStackFeeder(const TcpObjectStackFeeder&)            = delete;
    TcpObjectStackFeeder& operator=(const TcpObjectStackFeeder&) = delete;

    TcpObjectStackFeeder(TcpObjectStackFeeder&& o) noexcept : h_(o.h_) { o.h_ = nullptr; }
    TcpObjectStackFeeder& operator=(TcpObjectStackFeeder&& o) noexcept {
        if (this != &o) {
            reset();
            h_   = o.h_;
            o.h_ = nullptr;
        }
        return *this;
    }

    ~TcpObjectStackFeeder() { reset(); }

    bool             write(const uint8_t* data, std::size_t len, FfiError& err) const;
    ::TcpFeedObject* raw() const { return h_; }

  private:
    friend class TcpObjectStack;
    explicit TcpObjectStackFeeder(::TcpFeedObject* h) : h_(h) {}

    void reset() {
        if (h_) {
            ::idevice_free_tcp_feed_object(h_);
            h_ = nullptr;
        }
    }

    ::TcpFeedObject* h_ = nullptr;
};

// ---------------- TcpEater: blocking read of outbound packets ----------------
class TcpObjectStackEater {
  public:
    TcpObjectStackEater()                                      = default;
    TcpObjectStackEater(const TcpObjectStackEater&)            = delete;
    TcpObjectStackEater& operator=(const TcpObjectStackEater&) = delete;

    TcpObjectStackEater(TcpObjectStackEater&& o) noexcept : h_(o.h_) { o.h_ = nullptr; }
    TcpObjectStackEater& operator=(TcpObjectStackEater&& o) noexcept {
        if (this != &o) {
            reset();
            h_   = o.h_;
            o.h_ = nullptr;
        }
        return *this;
    }

    ~TcpObjectStackEater() { reset(); }

    // Blocks until a packet is available. On success, 'out' adopts the buffer
    // and you must keep 'out' alive until done (RAII frees via idevice_data_free).
    bool            read(OwnedBuffer& out, FfiError& err) const;

    ::TcpEatObject* raw() const { return h_; }

  private:
    friend class TcpObjectStack;
    explicit TcpObjectStackEater(::TcpEatObject* h) : h_(h) {}

    void reset() {
        if (h_) {
            ::idevice_free_tcp_eat_object(h_);
            h_ = nullptr;
        }
    }

    ::TcpEatObject* h_ = nullptr;
};

// ---------------- Stack builder: returns feeder + eater + adapter ------------
class TcpObjectStack {
  public:
    TcpObjectStack()                                     = default;
    TcpObjectStack(const TcpObjectStack&)                = delete; // no sharing
    TcpObjectStack& operator=(const TcpObjectStack&)     = delete;
    TcpObjectStack(TcpObjectStack&&) noexcept            = default; // movable
    TcpObjectStack& operator=(TcpObjectStack&&) noexcept = default;

    // Build the stack (dual-handle). Name kept to minimize churn.
    static std::optional<TcpObjectStack>
    create(const std::string& our_ip, const std::string& their_ip, FfiError& err);

    TcpObjectStackFeeder&               feeder();
    const TcpObjectStackFeeder&         feeder() const;

    TcpObjectStackEater&                eater();
    const TcpObjectStackEater&          eater() const;

    Adapter&                            adapter();
    const Adapter&                      adapter() const;

    std::optional<TcpObjectStackFeeder> release_feeder(); // nullptr inside wrapper after call
    std::optional<TcpObjectStackEater>  release_eater();  // nullptr inside wrapper after call
    std::optional<Adapter>              release_adapter();

  private:
    struct Impl {
        TcpObjectStackFeeder   feeder;
        TcpObjectStackEater    eater;
        std::optional<Adapter> adapter;
    };
    // Unique ownership so there’s a single point of truth to release from
    std::unique_ptr<Impl> impl_;
};

} // namespace IdeviceFFI
