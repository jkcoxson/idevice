# Jackson Coxson

cmake_minimum_required(VERSION 3.15)
project(IdeviceFFI CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin)

# Paths
set(IDEVICE_CPP_INCLUDE_DIR ${CMAKE_SOURCE_DIR}/../include)   # public C++ headers
set(IDEVICE_CPP_SRC_DIR     ${CMAKE_SOURCE_DIR}/../src)       # C++ .cpp files
set(IDEVICE_FFI_INCLUDE_DIR ${CMAKE_SOURCE_DIR}/../../ffi)    # Rust FFI headers (idevice.h)
set(PLIST_CPP_INCLUDE_DIR   ${CMAKE_SOURCE_DIR}/../plist_ffi/cpp/include)
set(PLIST_CPP_SRC_DIR       ${CMAKE_SOURCE_DIR}/../plist_ffi/cpp/src)
if (MSVC)
  set(STATIC_LIB ${CMAKE_SOURCE_DIR}/../../target/release/idevice_ffi.lib)
else()
  set(STATIC_LIB ${CMAKE_SOURCE_DIR}/../../target/release/libidevice_ffi.a)
endif()
set(EXAMPLES_DIR ${CMAKE_SOURCE_DIR}/../examples)
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin)

# Warnings
if (MSVC)
  add_compile_options(/W4 /permissive- /EHsc)
else()
  add_compile_options(-Wall -Wextra -Wpedantic)
endif()

# ---- Build the C++ wrapper library -----------------------------------------

# Collect your .cpps
file(GLOB IDEVICE_CPP_SOURCES
  ${IDEVICE_CPP_SRC_DIR}/*.cpp
)

file(GLOB PLIST_CPP_SOURCES
  ${PLIST_CPP_SRC_DIR}/*.cpp
)

add_library(idevice_cpp STATIC ${IDEVICE_CPP_SOURCES} ${PLIST_CPP_SOURCES})

target_include_directories(idevice_cpp
  PUBLIC
    ${IDEVICE_CPP_INCLUDE_DIR}
  PRIVATE
    ${IDEVICE_FFI_INCLUDE_DIR}
    ${PLIST_CPP_INCLUDE_DIR}
)

# Link to the Rust static lib (+ system libs/frameworks). Mark as PUBLIC so dependents inherit.
target_link_libraries(idevice_cpp
  PUBLIC
    ${STATIC_LIB}
)

# Unix-y extras frequently required by Rust static libs
if (UNIX AND NOT APPLE)
  find_package(Threads REQUIRED)
  target_link_libraries(idevice_cpp PUBLIC Threads::Threads dl m)
endif()

# Apple frameworks (propagate to dependents)
if (APPLE)
  target_link_libraries(idevice_cpp PUBLIC
    "-framework CoreFoundation"
    "-framework Security"
    "-framework SystemConfiguration"
    "-framework CoreServices"
    "-framework IOKit"
    "-framework CFNetwork"
  )
endif()

if (WIN32)
  target_link_libraries(idevice_cpp PUBLIC
    ws2_32
    userenv
    ntdll
    bcrypt
  )
endif()

# Optional: position independent code if you later turn idevice_cpp into a shared lib
set_target_properties(idevice_cpp PROPERTIES POSITION_INDEPENDENT_CODE ON)

# ---- Build each example and link against idevice_cpp ------------------------

file(GLOB EXAMPLE_SOURCES ${EXAMPLES_DIR}/*.cpp)

foreach(EXAMPLE_FILE ${EXAMPLE_SOURCES})
  get_filename_component(EXAMPLE_NAME ${EXAMPLE_FILE} NAME_WE)
  add_executable(${EXAMPLE_NAME} ${EXAMPLE_FILE})

  # Examples include public headers and (if they directly include FFI headers) the FFI dir.
  target_include_directories(${EXAMPLE_NAME} PRIVATE
    ${IDEVICE_CPP_INCLUDE_DIR}
    ${IDEVICE_FFI_INCLUDE_DIR}
    ${PLIST_CPP_INCLUDE_DIR}
  )

  # Link to your C++ wrapper (inherits Rust lib + frameworks/system libs)
  target_link_libraries(${EXAMPLE_NAME} PRIVATE idevice_cpp)
endforeach()

