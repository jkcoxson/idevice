// Jackson Coxson

#pragma once

#include <cstdio>
#include <exception>
#include <type_traits>
#include <utility>

namespace IdeviceFFI {
namespace types {
template <typename T> struct Ok {
    T val;

    Ok(const T& val) : val(val) {}
    Ok(T&& val) : val(std::move(val)) {}
};

template <> struct Ok<void> {};

template <typename E> struct Err {
    E val;

    Err(const E& val) : val(val) {}
    Err(E&& val) : val(std::move(val)) {}
};
} // namespace types

template <typename T> inline types::Ok<typename std::decay<T>::type> Ok(T&& val) {
    return types::Ok<typename std::decay<T>::type>(std::forward<T>(val));
}

inline types::Ok<void> Ok() {
    return types::Ok<void>();
}

template <typename E> inline types::Err<typename std::decay<E>::type> Err(E&& val) {
    return types::Err<typename std::decay<E>::type>(std::forward<E>(val));
}

// =======================
// Result<T, E>
// =======================
template <typename T, typename E> class Result {
    bool is_ok_;
    union {
        T ok_value_;
        E err_value_;
    };

  public:
    Result(types::Ok<T> ok_val) : is_ok_(true), ok_value_(std::move(ok_val.val)) {}
    Result(types::Err<E> err_val) : is_ok_(false), err_value_(std::move(err_val.val)) {}

    Result(const Result& other) : is_ok_(other.is_ok_) {
        if (is_ok_) {
            new (&ok_value_) T(other.ok_value_);
        } else {
            new (&err_value_) E(other.err_value_);
        }
    }

    Result(Result&& other) noexcept : is_ok_(other.is_ok_) {
        if (is_ok_) {
            new (&ok_value_) T(std::move(other.ok_value_));
        } else {
            new (&err_value_) E(std::move(other.err_value_));
        }
    }

    ~Result() {
        if (is_ok_) {
            ok_value_.~T();
        } else {
            err_value_.~E();
        }
    }

    // Copy Assignment
    Result& operator=(const Result& other) {
        // Prevent self-assignment
        if (this == &other) {
            return *this;
        }

        // Destroy the current value
        if (is_ok_) {
            ok_value_.~T();
        } else {
            err_value_.~E();
        }

        is_ok_ = other.is_ok_;

        // Construct the new value
        if (is_ok_) {
            new (&ok_value_) T(other.ok_value_);
        } else {
            new (&err_value_) E(other.err_value_);
        }

        return *this;
    }

    // Move Assignment
    Result& operator=(Result&& other) noexcept {
        if (this == &other) {
            return *this;
        }

        // Destroy the current value
        if (is_ok_) {
            ok_value_.~T();
        } else {
            err_value_.~E();
        }

        is_ok_ = other.is_ok_;

        // Construct the new value by moving
        if (is_ok_) {
            new (&ok_value_) T(std::move(other.ok_value_));
        } else {
            new (&err_value_) E(std::move(other.err_value_));
        }

        return *this;
    }

    bool is_ok() const { return is_ok_; }
    bool is_err() const { return !is_ok_; }

    // lvalue (mutable)
    T&   unwrap() & {
        if (!is_ok_) {
            std::fprintf(stderr, "unwrap on Err\n");
            std::terminate();
        }
        return ok_value_;
    }

    // lvalue (const)
    const T& unwrap() const& {
        if (!is_ok_) {
            std::fprintf(stderr, "unwrap on Err\n");
            std::terminate();
        }
        return ok_value_;
    }

    // rvalue (consume/move)
    T unwrap() && {
        if (!is_ok_) {
            std::fprintf(stderr, "unwrap on Err\n");
            std::terminate();
        }
        return std::move(ok_value_);
    }

    E& unwrap_err() & {
        if (is_ok_) {
            std::fprintf(stderr, "unwrap_err on Ok\n");
            std::terminate();
        }
        return err_value_;
    }

    const E& unwrap_err() const& {
        if (is_ok_) {
            std::fprintf(stderr, "unwrap_err on Ok\n");
            std::terminate();
        }
        return err_value_;
    }

    E unwrap_err() && {
        if (is_ok_) {
            std::fprintf(stderr, "unwrap_err on Ok\n");
            std::terminate();
        }
        return std::move(err_value_);
    }

    T unwrap_or(T&& default_value) const { return is_ok_ ? ok_value_ : std::move(default_value); }

    T expect(const char* message) && {
        if (is_err()) {
            std::fprintf(stderr, "Fatal (expect) error: %s\n", message);
            std::terminate();
        }
        return std::move(ok_value_);
    }

    // Returns a mutable reference from an lvalue Result
    T& expect(const char* message) & {
        if (is_err()) {
            std::fprintf(stderr, "Fatal (expect) error: %s\n", message);
            std::terminate();
        }
        return ok_value_;
    }

    // Returns a const reference from a const lvalue Result
    const T& expect(const char* message) const& {
        if (is_err()) {
            std::fprintf(stderr, "Fatal (expect) error: %s\n", message);
            std::terminate();
        }
        return ok_value_;
    }

    template <typename F> T unwrap_or_else(F&& f) & {
        return is_ok_ ? ok_value_ : static_cast<T>(f(err_value_));
    }

    // const lvalue: returns T by copy
    template <typename F> T unwrap_or_else(F&& f) const& {
        return is_ok_ ? ok_value_ : static_cast<T>(f(err_value_));
    }

    // rvalue: moves Ok(T) out; on Err(E), allow the handler to consume/move E
    template <typename F> T unwrap_or_else(F&& f) && {
        if (is_ok_) {
            return std::move(ok_value_);
        }
        return static_cast<T>(std::forward<F>(f)(std::move(err_value_)));
    }
};

// Result<void, E> specialization

template <typename E> class Result<void, E> {
    bool is_ok_;
    union {
        char dummy_;
        E    err_value_;
    };

  public:
    Result(types::Ok<void>) : is_ok_(true), dummy_() {}
    Result(types::Err<E> err_val) : is_ok_(false), err_value_(std::move(err_val.val)) {}

    Result(const Result& other) : is_ok_(other.is_ok_) {
        if (!is_ok_) {
            new (&err_value_) E(other.err_value_);
        }
    }

    Result(Result&& other) noexcept : is_ok_(other.is_ok_) {
        if (!is_ok_) {
            new (&err_value_) E(std::move(other.err_value_));
        }
    }

    ~Result() {
        if (!is_ok_) {
            err_value_.~E();
        }
    }

    bool is_ok() const { return is_ok_; }
    bool is_err() const { return !is_ok_; }

    void unwrap() const {
        if (!is_ok_) {
            std::fprintf(stderr, "Attempted to unwrap an error Result<void, E>\n");
            std::terminate();
        }
    }

    const E& unwrap_err() const {
        if (is_ok_) {
            std::fprintf(stderr, "Attempted to unwrap_err on an ok Result<void, E>\n");
            std::terminate();
        }
        return err_value_;
    }

    E& unwrap_err() {
        if (is_ok_) {
            std::fprintf(stderr, "Attempted to unwrap_err on an ok Result<void, E>\n");
            std::terminate();
        }
        return err_value_;
    }

    void expect(const char* message) const {
        if (is_err()) {
            std::fprintf(stderr, "Fatal (expect) error: %s\n", message);
            std::terminate();
        }
    }
};

#define match_result(res, ok_name, ok_block, err_name, err_block)                                  \
    do {                                                                                           \
        auto&& _result_val = (res);                                                                \
        if (_result_val.is_ok()) {                                                                 \
            auto&& ok_name = _result_val.unwrap();                                                 \
            ok_block                                                                               \
        } else {                                                                                   \
            auto&& err_name = _result_val.unwrap_err();                                            \
            err_block                                                                              \
        }                                                                                          \
    } while (0)
} // namespace IdeviceFFI
